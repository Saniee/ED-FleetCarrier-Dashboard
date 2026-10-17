//! The EDMC plugin, embedded at build time (see `build.rs`) so the download
//! always matches the server it came from.

use std::{
    io::{Cursor, Write},
    sync::LazyLock,
};

use axum::{
    Json,
    http::{StatusCode, header},
    response::IntoResponse,
};
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/plugin.rs"));
}

/// The zip, built once: a single folder named after the plugin, ready to drop
/// into EDMC's plugins directory.
static ZIP: LazyLock<Result<Vec<u8>, String>> = LazyLock::new(|| {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (path, bytes) in embedded::FILES {
        zip.start_file(format!("{}/{path}", embedded::NAME), options)
            .and_then(|()| zip.write_all(bytes).map_err(Into::into))
            .map_err(|e| e.to_string())?;
    }
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
});

fn file_name() -> String {
    format!("{}-{}.zip", embedded::NAME, embedded::VERSION)
}

/// `{ name, version, file, size }` for the download link.
pub async fn info() -> Result<Json<Value>, StatusCode> {
    let zip = ZIP.as_ref().map_err(crate::error::internal)?;
    Ok(Json(json!({
        "name": embedded::NAME,
        "version": embedded::VERSION,
        "file": file_name(),
        "size": zip.len(),
    })))
}

pub async fn download() -> Result<impl IntoResponse, StatusCode> {
    let zip = ZIP.as_ref().map_err(crate::error::internal)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", file_name())),
        ],
        zip.as_slice(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn zip_holds_the_plugin_under_one_folder() {
        let bytes = ZIP.as_ref().unwrap().clone();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
        let root = format!("{}/", embedded::NAME);
        assert!(names.iter().all(|n| n.starts_with(&root)), "{names:?}");
        assert!(names.iter().all(|n| !n.contains("__pycache__")), "{names:?}");

        let mut load = String::new();
        archive.by_name(&format!("{root}load.py")).unwrap().read_to_string(&mut load).unwrap();
        assert!(load.contains("def plugin_start3"));
        let mut common = String::new();
        archive.by_name(&format!("{root}edc/common.py")).unwrap().read_to_string(&mut common).unwrap();
        assert!(common.contains(&format!("__version__ = \"{}\"", embedded::VERSION)));
    }
}
