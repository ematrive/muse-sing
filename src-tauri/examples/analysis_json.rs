fn main() {
    let p = std::env::args().nth(1).unwrap();
    let a = muse_sing_lib::commands::analyze(p).unwrap();
    println!("{}", serde_json::to_string(&a).unwrap());
}
