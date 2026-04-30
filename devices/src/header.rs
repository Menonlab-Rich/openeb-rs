use crate::types::{DeviceFileError, FileFormat};
use openeb_core::hal::facilities::SensorInfo;
use std::collections::HashMap;
use std::io::BufRead;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub format: FileFormat,
    pub width: u32,
    pub height: u32,
    pub metadata: HashMap<String, String>,
}

impl Header {
    pub fn parse<R: BufRead>(reader: &mut R) -> Result<Header, DeviceFileError> {
        let mut metadata = HashMap::new();

        loop {
            let buf = reader.fill_buf()?;
            if buf.is_empty() || buf[0] != b'%' {
                break;
            }

            let mut line = String::new();
            reader.read_line(&mut line)?;

            let line = line.trim();
            if let Some(rest) = line.strip_prefix('%') {
                let parts: Vec<&str> = rest.splitn(2, ':').collect();
                if parts.len() == 2 {
                    metadata.insert(parts[0].trim().to_string(), parts[1].trim().to_string());
                } else {
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if parts.len() >= 2 {
                        metadata.insert(parts[0].to_string(), parts[1..].join(" "));
                    }
                }
            }
        }

        let raw_format_str = metadata
            .get("Data format")
            .or_else(|| metadata.get("format"))
            .map(|s| s.as_str())
            .unwrap_or("UNKNOWN");

        let parts = raw_format_str.splitn(3, ';');
        let mut format_str: Option<String> = None;
        let mut width: Option<String> = None;
        let mut height: Option<String> = None;
        for (i, p) in parts.enumerate() {
            match i {
                0 => format_str = Some(p.to_string()),
                1 => width = Some(p.to_string()),
                2 => height = Some(p.to_string()),
                _ => panic!("This should never happen"),
            }
        }

        let fmt_str = format_str.unwrap_or_else(|| raw_format_str.to_string());

        let format = match fmt_str.as_str() {
            "EVT2" => FileFormat::EVT2,
            "EVT3" => FileFormat::EVT3,
            "DAT" => FileFormat::DAT,
            "HDF5" => FileFormat::HDF5,
            _ => FileFormat::UNKNOWN,
        };

        if let Some(w) = width {
            metadata.insert("Geometry".to_string(), format!("{},{}", w, height.unwrap()));
        }

        let geometry_str = metadata
            .get("Geometry")
            .ok_or_else(|| metadata.get("geometry"))
            .or(Err(DeviceFileError::UnknownGeometry()))?;

        let coords = {
            if geometry_str.contains("=") {
                geometry_str
                    .split(',')
                    .try_fold(HashMap::<&str, &str>::new(), |mut acc, s| {
                        let parts: Vec<&str> = s.split("=").collect();
                        if parts.len() != 2 {
                            return Err(DeviceFileError::UnknownGeometry());
                        }
                        acc.insert(parts[0], parts[1]);
                        Ok(acc)
                    })
            } else {
                let mut coord_map = HashMap::<&str, &str>::new();
                let coord_values: Vec<&str> = geometry_str.split(',').collect();
                if coord_values.len() != 2 {
                    return Err(DeviceFileError::UnknownGeometry());
                }
                coord_map.insert("width", coord_values[0]);
                coord_map.insert("height", coord_values[1]);

                Ok(coord_map)
            }
        }?;
        if coords.len() != 2 {
            return Err(DeviceFileError::UnknownGeometry());
        }

        let width = coords
            .get("width")
            .ok_or(DeviceFileError::UnknownGeometry())?
            .parse::<u32>()?;
        let height = coords
            .get("height")
            .ok_or(DeviceFileError::UnknownGeometry())?
            .parse::<u32>()?;

        Ok(Header {
            format,
            width,
            height,
            metadata,
        })
    }
}

pub fn sensor_info_from_header(header: &Header) -> SensorInfo {
    let name = header
        .metadata
        .get("sensor_name")
        .map_or("UNKNOWN".to_string(), |v| v.to_string());
    let integrator = header
        .metadata
        .get("integrator_name")
        .map_or("UNKNOWN".to_string(), |v| v.to_string());
    let version = header
        .metadata
        .get("sensor_generation")
        .or_else(|| header.metadata.get("generation"))
        .map_or("x.x".to_string(), |v| v.to_string());

    SensorInfo {
        name,
        integrator,
        version,
    }
}
