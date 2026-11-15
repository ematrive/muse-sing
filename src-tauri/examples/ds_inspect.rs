//! Lists inputs and outputs of every .onnx file under a folder (development aid).
fn main() -> ort::Result<()> {
    let dir = std::env::args().nth(1).expect("usage: ds_inspect <folder>");
    let env = ort::init().build()?;
    let mut files = vec![];
    let mut stack = vec![std::path::PathBuf::from(dir)];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() { stack.push(p) } else if p.extension().is_some_and(|x| x == "onnx") { files.push(p) }
        }
    }
    files.sort();
    for f in files {
        println!("== {}", f.display());
        let s = ort::session::Session::builder(&env)?.with_auto_device(ort::session::builder::AutoDevicePolicy::PreferCPU)?.commit_from_file(&f)?;
        for i in s.inputs() { println!("  in  {} {:?}", i.name(), i.dtype()); }
        for o in s.outputs() { println!("  out {} {:?}", o.name(), o.dtype()); }
    }
    Ok(())
}
