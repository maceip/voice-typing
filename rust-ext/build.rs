use std::io::Write;
use std::path::Path;

fn main() {
    let out = std::env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out);

    pack_zip(
        "chrome",
        &[
            "manifest.json",
            "background.js",
            "content.js",
            "content.css",
            "icons/icon16.png",
            "icons/icon32.png",
            "icons/icon48.png",
            "icons/icon128.png",
        ],
        out_path,
    );
    pack_zip(
        "safari",
        &[
            "manifest.json",
            "background.js",
            "content.js",
            "content.css",
            "Info.plist",
            "icons/icon16.png",
            "icons/icon32.png",
            "icons/icon48.png",
            "icons/icon128.png",
        ],
        out_path,
    );

    // Expose OUT_DIR to dependents via DEP_DAYDREAM_EXT_OUT_DIR
    println!("cargo:OUT_DIR={out}");
    println!("cargo:rerun-if-changed=chrome");
    println!("cargo:rerun-if-changed=safari");
}

fn pack_zip(name: &str, files: &[&str], out_dir: &Path) {
    let zip_path = out_dir.join(format!("{name}.zip"));
    let file = std::fs::File::create(&zip_path).expect("create zip");
    let mut zip = zip::ZipWriter::new(file);

    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for file_name in files {
        let src = Path::new(name).join(file_name);
        if src.exists() {
            let contents = std::fs::read(&src).expect("read extension file");
            zip.start_file(*file_name, opts).expect("zip entry");
            zip.write_all(&contents).expect("zip write");
        }
    }

    zip.finish().expect("finish zip");
}
