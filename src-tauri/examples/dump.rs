fn main() {
    let a: Vec<String> = std::env::args().collect();
    let s = muse_sing_lib::score::read_mscz(std::path::Path::new(&a[1])).unwrap();
    println!("warnings: {:?}", s.warnings);
    for p in s.parts.iter().filter(|p| p.is_sung()) {
        let l = muse_sing_lib::lyrics::word_lyrics(&p.notes, muse_sing_lib::lyrics::detect_lang(&s.parts), &Default::default());
        let words: Vec<&String> = l.iter().filter(|x| !x.starts_with('+')).collect();
        println!("{} ({} note): {}", p.name, p.notes.len(), words.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "));
    }
}
