/// Minimal NetCDF3 Classic reader — pure Rust, no C dependencies.
/// Handles record variables (unlimited dimension) correctly.

use std::io::{Read, Seek, SeekFrom};

const NC_DIMENSION: u32 = 0x0000000A;
const NC_VARIABLE: u32 = 0x0000000B;
const NC_ATTRIBUTE: u32 = 0x0000000C;
// const NC_CHAR: u32 = 2;
const NC_FLOAT: u32 = 5;
const NC_DOUBLE: u32 = 6;

fn type_size(t: u32) -> usize {
    match t { 1 => 1, 2 => 1, 3 => 2, 4 => 4, 5 => 4, 6 => 8, _ => 0 }
}

fn pad4(n: usize) -> usize { (n + 3) & !3 }

fn read_u32<R: Read>(r: &mut R) -> std::io::Result<u32> {
    let mut b = [0u8; 4]; r.read_exact(&mut b)?; Ok(u32::from_be_bytes(b))
}

fn read_name<R: Read>(r: &mut R) -> std::io::Result<String> {
    let len = read_u32(r)? as usize;
    let padded = pad4(len);
    let mut buf = vec![0u8; padded];
    r.read_exact(&mut buf)?;
    buf.truncate(len);
    Ok(String::from_utf8_lossy(&buf).to_string())
}

fn skip_att_list<R: Read>(r: &mut R) -> std::io::Result<()> {
    let tag = read_u32(r)?;
    let nelems = read_u32(r)?;
    if tag == 0 && nelems == 0 { return Ok(()); }
    if tag != NC_ATTRIBUTE { return Ok(()); }
    for _ in 0..nelems {
        let _name = read_name(r)?;
        let nc_type = read_u32(r)?;
        let count = read_u32(r)?;
        let bytes = pad4(count as usize * type_size(nc_type));
        let mut buf = vec![0u8; bytes];
        r.read_exact(&mut buf)?;
    }
    Ok(())
}

fn skip_att_value<R: Read>(r: &mut R, nc_type: u32, nelems: u32) -> std::io::Result<()> {
    let bytes = pad4(nelems as usize * type_size(nc_type));
    let mut buf = vec![0u8; bytes];
    r.read_exact(&mut buf)?;
    Ok(())
}

fn read_fill_value<R: Read>(r: &mut R, nc_type: u32, nelems: u32) -> std::io::Result<f64> {
    if nc_type == NC_FLOAT && nelems == 1 {
        let mut b = [0u8; 4];
        r.read_exact(&mut b)?;
        // no padding needed for 4 bytes
        Ok(f32::from_be_bytes(b) as f64)
    } else if nc_type == NC_DOUBLE && nelems == 1 {
        let mut b = [0u8; 8];
        r.read_exact(&mut b)?;
        Ok(f64::from_be_bytes(b))
    } else {
        skip_att_value(r, nc_type, nelems)?;
        Ok(f64::NAN)
    }
}

fn read_var_attrs<R: Read>(r: &mut R) -> std::io::Result<f64> {
    let tag = read_u32(r)?;
    let nelems = read_u32(r)?;
    if tag == 0 && nelems == 0 { return Ok(-9999.99); }
    if tag != NC_ATTRIBUTE { return Ok(-9999.99); }
    let mut fill = -9999.99_f64;
    for _ in 0..nelems {
        let name = read_name(r)?;
        let nc_type = read_u32(r)?;
        let count = read_u32(r)?;
        if name == "_FillValue" {
            fill = read_fill_value(r, nc_type, count)?;
        } else {
            skip_att_value(r, nc_type, count)?;
        }
    }
    Ok(fill)
}

struct VarInfo {
    name: String,
    dim_ids: Vec<u32>,
    nc_type: u32,
    vsize: u32,   // size per record (for record vars) or total size
    offset: u64,  // begin offset
    fill: f64,
    is_record: bool,
}

pub struct Nc3File {
    path: String,
    vars: Vec<VarInfo>,
    dims: Vec<(String, u32)>,
    numrecs: u32,
    #[allow(dead_code)]
    unlim_dim: Option<usize>,
    recsize: u64, // total record size (sum of all record var vsizes)
}

pub struct Nc3Variable {
    pub values: Vec<f64>,
}

impl Nc3File {
    pub fn open(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut f = std::fs::File::open(path)?;

        let mut magic = [0u8; 4];
        f.read_exact(&mut magic)?;
        if &magic[..3] != b"CDF" {
            return Err("Not a NetCDF file".into());
        }
        let is_64bit = magic[3] == 2;

        let numrecs = read_u32(&mut f)?;

        // dim_list
        let mut dims: Vec<(String, u32)> = Vec::new();
        let mut unlim_dim: Option<usize> = None;
        let dim_tag = read_u32(&mut f)?;
        let dim_count = read_u32(&mut f)?;
        if dim_tag == NC_DIMENSION {
            for i in 0..dim_count {
                let name = read_name(&mut f)?;
                let len = read_u32(&mut f)?;
                if len == 0 { unlim_dim = Some(i as usize); }
                dims.push((name, len));
            }
        }

        // global attrs
        skip_att_list(&mut f)?;

        // var_list
        let mut vars: Vec<VarInfo> = Vec::new();
        let var_tag = read_u32(&mut f)?;
        let var_count = read_u32(&mut f)?;
        if var_tag == NC_VARIABLE {
            for _ in 0..var_count {
                let name = read_name(&mut f)?;
                let ndims = read_u32(&mut f)?;
                let mut dim_ids = Vec::new();
                for _ in 0..ndims {
                    dim_ids.push(read_u32(&mut f)?);
                }
                let fill = read_var_attrs(&mut f)?;
                let nc_type = read_u32(&mut f)?;
                let vsize = read_u32(&mut f)?;
                let offset = if is_64bit {
                    let mut b = [0u8; 8];
                    f.read_exact(&mut b)?;
                    u64::from_be_bytes(b)
                } else {
                    read_u32(&mut f)? as u64
                };
                let is_record = !dim_ids.is_empty() && unlim_dim == Some(dim_ids[0] as usize);
                vars.push(VarInfo { name, dim_ids, nc_type, vsize, offset, fill, is_record });
            }
        }

        // Calculate record size
        let recsize: u64 = vars.iter()
            .filter(|v| v.is_record)
            .map(|v| v.vsize as u64)
            .sum();

        Ok(Nc3File { path: path.to_string(), vars, dims, numrecs, unlim_dim, recsize })
    }

    pub fn read_var(&self, name: &str) -> Result<Nc3Variable, Box<dyn std::error::Error>> {
        let var = self.vars.iter().find(|v| v.name == name)
            .ok_or_else(|| format!("Variable '{}' not found", name))?;

        let mut f = std::fs::File::open(&self.path)?;

        if var.is_record {
            // Record variable: data is interleaved across records
            // For record i, data at: offset + i * recsize
            let nrecs = self.numrecs as usize;
            // Elements per record slice
            let elem_size = type_size(var.nc_type);
            let elems_per_rec = var.vsize as usize / elem_size;
            let total = nrecs * elems_per_rec;
            let mut values: Vec<f64> = Vec::with_capacity(total);

            for rec in 0..nrecs {
                let pos = var.offset + rec as u64 * self.recsize;
                f.seek(SeekFrom::Start(pos))?;

                match var.nc_type {
                    NC_FLOAT => {
                        let mut buf = vec![0u8; elems_per_rec * 4];
                        f.read_exact(&mut buf)?;
                        for chunk in buf.chunks_exact(4) {
                            let v = f32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                            if (v as f64 - var.fill).abs() < 0.01 || v < -9990.0 {
                                values.push(f64::NAN);
                            } else {
                                values.push(v as f64);
                            }
                        }
                    }
                    NC_DOUBLE => {
                        let mut buf = vec![0u8; elems_per_rec * 8];
                        f.read_exact(&mut buf)?;
                        for chunk in buf.chunks_exact(8) {
                            let v = f64::from_be_bytes([
                                chunk[0], chunk[1], chunk[2], chunk[3],
                                chunk[4], chunk[5], chunk[6], chunk[7],
                            ]);
                            if (v - var.fill).abs() < 0.01 || v < -9990.0 {
                                values.push(f64::NAN);
                            } else {
                                values.push(v);
                            }
                        }
                    }
                    _ => return Err(format!("Unsupported type {} for '{}'", var.nc_type, name).into()),
                }
            }
            Ok(Nc3Variable { values })
        } else {
            // Non-record variable: contiguous data at offset
            let total: usize = var.dim_ids.iter()
                .map(|&d| self.dims[d as usize].1 as usize)
                .product();
            f.seek(SeekFrom::Start(var.offset))?;
            let mut values: Vec<f64> = Vec::with_capacity(total);

            match var.nc_type {
                NC_FLOAT => {
                    let mut buf = vec![0u8; total * 4];
                    f.read_exact(&mut buf)?;
                    for chunk in buf.chunks_exact(4) {
                        let v = f32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                        if (v as f64 - var.fill).abs() < 0.01 || v < -9990.0 {
                            values.push(f64::NAN);
                        } else {
                            values.push(v as f64);
                        }
                    }
                }
                NC_DOUBLE => {
                    let mut buf = vec![0u8; total * 8];
                    f.read_exact(&mut buf)?;
                    for chunk in buf.chunks_exact(8) {
                        let v = f64::from_be_bytes([
                            chunk[0], chunk[1], chunk[2], chunk[3],
                            chunk[4], chunk[5], chunk[6], chunk[7],
                        ]);
                        if (v - var.fill).abs() < 0.01 || v < -9990.0 {
                            values.push(f64::NAN);
                        } else {
                            values.push(v);
                        }
                    }
                }
                _ => return Err(format!("Unsupported type {} for '{}'", var.nc_type, name).into()),
            }
            Ok(Nc3Variable { values })
        }
    }
}
