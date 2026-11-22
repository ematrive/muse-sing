//! End-to-end render with a real voicebank: set MUSESING_TEST_VOICEBANK to a DiffSinger voicebank
//! folder (e.g. TIGER DS). Skipped otherwise, and when the score fixtures are missing.
use std::path::PathBuf;

use muse_sing_lib::lyrics::Lang;
use muse_sing_lib::mix::{self, MixPart};
use muse_sing_lib::sing::{PartReq, RenderRequest, Sing};
use muse_sing_lib::singers::SingerInfo;

#[test]
fn renders_mixes_and_reuses_the_cache() {
    let Some(vb) = std::env::var_os("MUSESING_TEST_VOICEBANK").map(PathBuf::from) else {
        eprintln!("MUSESING_TEST_VOICEBANK not set: skipped");
        return;
    };
    let score = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agnello.mscz");
    if !score.exists() {
        eprintln!("fixture missing: skipped");
        return;
    }
    let singer = SingerInfo {
        id: "test".into(),
        name: "test".into(),
        singer_type: "diffsinger".into(),
        langs: vec![],
        path: vb.to_string_lossy().into_owned(),
        openutau: false,
        engine: true,
        engine_note: None,
        colors: vec![],
    };
    let cache = std::env::temp_dir().join(format!("muse-sing-sing-{}", std::process::id()));
    let sing = Sing::new(Some(cache.clone()));
    let req = RenderRequest {
        path: score.to_string_lossy().into_owned(),
        lang: Lang::It,
        pitch: "notes".into(),
        parts: vec![PartReq { name: "Soprano".into(), singer: "test".into(), color: None, voices: 1, played: false }],
        tempo_factor: 1.0,
        accompaniment: false,
        edits: Default::default(),
        instruments: Default::default(),
        marks: true,
        transpose: 0,
        cache_only: false,
        soundfont: None,
    };
    let t = std::time::Instant::now();
    let job = sing.begin();
    let (warnings, secs) = sing.run(job, &req, std::slice::from_ref(&singer), &|_| {}).unwrap().expect("not cancelled");
    eprintln!("rendered {secs:.1}s in {:.1}s, warnings: {warnings:?}", t.elapsed().as_secs_f64());
    assert!(secs > 10.0);
    assert!(warnings.iter().all(|w| !w.contains("non riconosciuti")), "{warnings:?}");

    let part = MixPart { name: "Soprano".into(), volume_db: 0.0, pan: -0.5, mute: false, solo: false };
    let (l, r) = sing.with_tracks(|t| mix::mix(t, std::slice::from_ref(&part))).unwrap();
    let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
    assert!(rms(&l) > 0.01, "not silent");
    assert!(rms(&l) > rms(&r), "panned left");

    // second time from the cache, and a cancelled job leaves the tracks alone
    let t = std::time::Instant::now();
    let job = sing.begin();
    sing.run(job, &req, std::slice::from_ref(&singer), &|p| assert!(p.cached)).unwrap().unwrap();
    assert!(t.elapsed().as_secs_f64() < 5.0);
    let job = sing.begin();
    sing.cancel();
    assert!(sing.run(job, &req, std::slice::from_ref(&singer), &|_| {}).unwrap().is_none());

    // the app opened again (a new engine, same cache): the project's voices come back without
    // synthesizing; with other choices nothing is synthesized and the job ends as cancelled
    let again = Sing::new(Some(cache.clone()));
    let only = RenderRequest { cache_only: true, ..req.clone() };
    let job = again.begin();
    let (_, secs2) = again.run(job, &only, std::slice::from_ref(&singer), &|p| assert!(p.cached)).unwrap().expect("from the cache");
    assert_eq!(secs2, secs);
    assert!(again.with_tracks(|t| mix::mix(t, std::slice::from_ref(&part))).is_ok());
    let other = RenderRequest { pitch: "model".into(), ..only };
    let t = std::time::Instant::now();
    let job = again.begin();
    assert!(again.run(job, &other, std::slice::from_ref(&singer), &|p| assert!(p.cached)).unwrap().is_none());
    assert!(t.elapsed().as_secs_f64() < 5.0, "nothing synthesized");
    std::fs::remove_dir_all(cache).ok();
}

/// The voicebank is found in a Singers folder, usable by the engine, and Italian hints written in
/// G2p symbols come out as the same phonemes as plain words.
#[test]
fn finds_the_singer_and_remaps_italian_hints() {
    let Some(vb) = std::env::var_os("MUSESING_TEST_VOICEBANK").map(PathBuf::from) else {
        eprintln!("MUSESING_TEST_VOICEBANK not set: skipped");
        return;
    };
    let root = vb.parent().unwrap();
    let all = muse_sing_lib::singers::list_singers(Some(root));
    let s = all.iter().find(|s| std::path::Path::new(&s.path) == vb).expect("voicebank listed");
    eprintln!("{} engine={} note={:?} colors={:?} langs={:?}", s.name, s.engine, s.engine_note, s.colors, s.langs);
    assert!(s.engine, "{:?}", s.engine_note);
    assert!(s.id.starts_with(muse_sing_lib::singers::OWN_PREFIX) && !s.openutau);
    assert!(s.langs.iter().any(|l| l == "it"));

    use muse_sing_lib::ds::{phonemize::{phonemize, PNote}, time::TimeAxis, voicebank::Voicebank};
    let v = Voicebank::load(&vb).unwrap();
    let axis = TimeAxis::new(&[(0, 120.0)], 480);
    let note = |pos: i64, lyric: &str| PNote { id: String::new(), position: pos, duration: 480, tone: 64, lyric: lyric.into() };
    let words = |notes: &[PNote]| -> Vec<String> {
        let ph = phonemize(&v, Lang::It, notes, &axis, &v.speaker_suffix(None)).unwrap();
        assert!(ph.warnings.is_empty(), "{:?}", ph.warnings);
        ph.phones.iter().map(|p| p.symbol.clone()).collect()
    };
    let hinted = words(&[note(0, "terra[t e rr a]"), note(480, "+"), note(1440, "gloria[g l o r y a]"), note(1920, "+"), note(2400, "+")]);
    eprintln!("hinted: {hinted:?}");
    for s in ["tx", "lg", "aa", "ee", "oo"] {
        assert!(hinted.iter().any(|p| p == s), "{s} missing in {hinted:?}");
    }
    for s in ["t", "l", "a", "e", "o"] {
        assert!(!hinted.iter().any(|p| p == s), "{s} not remapped in {hinted:?}");
    }
    let plain = words(&[note(0, "signore"), note(480, "+"), note(960, "+")]);
    assert!(plain.iter().any(|p| p == "ny"), "{plain:?}");
}

/// The accompaniment alone (no singer needed) and the practice tempo: 80% lasts 1/0.8 as long.
#[test]
fn accompaniment_and_slower_tempo() {
    let score = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/santo.mscz");
    if !score.exists() {
        eprintln!("fixture missing: skipped");
        return;
    }
    let sing = Sing::new(None);
    let run = |tempo_factor: f64| {
        let req = RenderRequest {
            path: score.to_string_lossy().into_owned(),
            lang: Lang::It,
            pitch: "model".into(),
            parts: vec![],
            tempo_factor,
            accompaniment: true,
            edits: Default::default(),
            instruments: Default::default(),
            marks: true,
            transpose: 0,
            cache_only: false,
            soundfont: None,
        };
        let job = sing.begin();
        sing.run(job, &req, &[], &|_| {}).unwrap().expect("not cancelled").1
    };
    let normal = run(1.0);
    let slow = run(0.8);
    eprintln!("accompaniment {normal:.1}s, at 80% {slow:.1}s");
    assert!(normal > 30.0);
    assert!((slow / normal - 1.25).abs() < 0.02, "{slow} / {normal}");
    let organ = MixPart { name: muse_sing_lib::project::ACCOMPANIMENT.into(), volume_db: 0.0, pan: 0.0, mute: false, solo: false };
    let (l, _) = sing.with_tracks(|t| mix::mix(t, std::slice::from_ref(&organ))).unwrap();
    assert!(l.iter().any(|x| x.abs() > 0.01), "the organ plays");
}

/// Edits: the phonemes the editor shows, written back, sing the same; a pitch, timbre or timing
/// edit renders only its phrase again and lands on the same note at the practice tempo; the
/// dynamics need no new render.
#[test]
fn edits_render_only_what_changed() {
    use muse_sing_lib::edits::{Curve, PartEdits, WordEdit, CURVE_STEP};
    let Some(vb) = std::env::var_os("MUSESING_TEST_VOICEBANK").map(PathBuf::from) else {
        eprintln!("MUSESING_TEST_VOICEBANK not set: skipped");
        return;
    };
    let score = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agnello.mscz");
    if !score.exists() {
        eprintln!("fixture missing: skipped");
        return;
    }
    let singer = SingerInfo {
        id: "test".into(),
        name: "test".into(),
        singer_type: "diffsinger".into(),
        langs: vec![],
        path: vb.to_string_lossy().into_owned(),
        openutau: false,
        engine: true,
        engine_note: None,
        colors: vec![],
    };
    let cache = std::env::temp_dir().join(format!("muse-sing-edits-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);
    let sing = Sing::new(Some(cache.clone()));
    let part = "Soprano";
    let run = |edits: PartEdits, tempo_factor: f64| {
        let req = RenderRequest {
            path: score.to_string_lossy().into_owned(),
            lang: Lang::It,
            pitch: "model".into(),
            parts: vec![PartReq { name: part.into(), singer: "test".into(), color: None, voices: 1, played: false }],
            tempo_factor,
            accompaniment: false,
            edits: std::collections::HashMap::from([(part.to_string(), edits)]),
            instruments: Default::default(),
            marks: true,
            transpose: 0,
            cache_only: false,
            soundfont: None,
        };
        let job = sing.begin();
        sing.run(job, &req, std::slice::from_ref(&singer), &|_| {}).unwrap().expect("not cancelled");
        let info = sing.track_info(part).expect("track info");
        let samples = sing.with_tracks(|t| t[0].1.to_vec()).unwrap();
        (info, samples)
    };
    let phrase_files = || std::fs::read_dir(cache.join("render")).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with('p')).count();
    let pitch_at = |info: &muse_sing_lib::sing::TrackInfo, sec: f64| -> f32 {
        let seg = info.pitch.iter().find(|s| s.start <= sec && sec < s.start + s.step * s.midi.len() as f64).expect("sung there");
        seg.midi[((sec - seg.start) / seg.step) as usize]
    };
    let window = |x: &[f32], a: f64, b: f64| x[(a * 44100.0) as usize..(b * 44100.0) as usize].to_vec();
    let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();

    let (base, base_audio) = run(PartEdits::default(), 1.0);
    let n0 = phrase_files();
    let w = base.words.iter().filter(|w| w.phones.len() >= 3 && w.end - w.start > 0.6).nth(2).expect("a long word").clone();
    let mid = (w.start + w.end) / 2.0;
    let symbols: Vec<String> = w.phones.iter().map(|p| p.symbol.clone()).collect();
    eprintln!("word {} «{}» {:.2}-{:.2} s: {symbols:?}; {n0} phrases", w.id, w.word, w.start, w.end);

    // 1. the shown phonemes written back sing the same (same phrase, from the cache)
    let mut e = PartEdits::default();
    e.notes.insert(w.id.clone(), WordEdit { word: w.word.clone(), phonemes: Some(symbols.join(" ")), offsets: vec![] });
    let (info, _) = run(e, 1.0);
    let again = info.words.iter().find(|x| x.id == w.id).unwrap();
    assert_eq!(again.phones.iter().map(|p| p.symbol.clone()).collect::<Vec<_>>(), symbols);
    assert_eq!(phrase_files(), n0, "nothing rendered again");

    // the word's span in score ticks (written tempo)
    let sc = muse_sing_lib::score::read_mscz(&score).unwrap();
    let names = [part.to_string()];
    let hints = std::collections::HashMap::new();
    let project = muse_sing_lib::ustx::build(
        &sc,
        &muse_sing_lib::ustx::Options { lang: Lang::It, singer: "", part_singers: None, parts: Some(&names), hints: &hints, fallback_name: "x" },
    );
    let axis = muse_sing_lib::ds::project_axis(&project);
    let (t0, t1) = (axis.ms_to_tick(w.start * 1000.0), axis.ms_to_tick(w.end * 1000.0));
    let over = |v: f32| Curve((t0 / CURVE_STEP..=t1 / CURVE_STEP).map(|k| (k * CURVE_STEP, v)).collect());

    // 2. +100 cents on the word: one phrase again, a semitone up
    let e = PartEdits { pitch: over(100.0), ..PartEdits::default() };
    let (info, _) = run(e.clone(), 1.0);
    let up = pitch_at(&info, mid) - pitch_at(&base, mid);
    eprintln!("pitch +100 cents: {up:+.3} semitones");
    assert!((up - 1.0).abs() < 0.02, "{up}");
    assert_eq!(phrase_files(), n0 + 1, "only the edited phrase");

    // 3. at 80% the same edit is on the same word
    let (slow_base, _) = run(PartEdits::default(), 0.8);
    let (slow, _) = run(e, 0.8);
    let up = pitch_at(&slow, mid / 0.8) - pitch_at(&slow_base, mid / 0.8);
    eprintln!("at 80%: {up:+.3} semitones");
    assert!((up - 1.0).abs() < 0.02, "{up}");

    // 4. dynamics: -6 dB, no new phrase
    let n = phrase_files();
    let (_, quiet) = run(PartEdits { dynamics: over(-6.0), ..PartEdits::default() }, 1.0);
    let ratio = rms(&window(&quiet, mid - 0.1, mid + 0.1)) / rms(&window(&base_audio, mid - 0.1, mid + 0.1));
    eprintln!("dynamics -6 dB: x{ratio:.3}");
    assert!((ratio - 0.501).abs() < 0.02, "{ratio}");
    assert_eq!(phrase_files(), n);

    // 5. timbre and 6. consonant speed: one phrase again each, the sound changes. (On TIGER the
    // timbre makes little difference up to ±0.3 and degrades the voice beyond: not checked here.)
    for e in [PartEdits { gender: over(0.3), ..PartEdits::default() }, PartEdits { velocity: over(0.5), ..PartEdits::default() }] {
        let n = phrase_files();
        let (_, x) = run(e, 1.0);
        assert_eq!(phrase_files(), n + 1);
        let a = window(&x, w.start, w.end);
        let b = window(&base_audio, w.start, w.end);
        assert!(a.iter().zip(&b).any(|(p, q)| (p - q).abs() > 1e-3), "changed");
    }

    // 7. timing: the second phoneme 40 ms earlier
    let mut offsets = vec![0.0; symbols.len()];
    offsets[1] = -40.0;
    let mut e = PartEdits::default();
    e.notes.insert(w.id.clone(), WordEdit { word: w.word.clone(), phonemes: None, offsets });
    let (info, _) = run(e, 1.0);
    let moved = &info.words.iter().find(|x| x.id == w.id).unwrap().phones[1];
    eprintln!("phoneme {} from {:.3} to {:.3}", moved.symbol, moved.base, moved.start);
    assert!((moved.start - (moved.base - 0.040)).abs() < 0.003);

    // 8. an edit made for another word (the score changed under it) is not applied
    let mut e = PartEdits::default();
    e.notes.insert(w.id.clone(), WordEdit { word: "altra".into(), phonemes: Some("a a a".into()), offsets: vec![] });
    let req = RenderRequest {
        path: score.to_string_lossy().into_owned(),
        lang: Lang::It,
        pitch: "model".into(),
        parts: vec![PartReq { name: part.into(), singer: "test".into(), color: None, voices: 1, played: false }],
        tempo_factor: 1.0,
        accompaniment: false,
        edits: std::collections::HashMap::from([(part.to_string(), e)]),
        instruments: Default::default(),
        marks: true,
        transpose: 0,
        cache_only: false,
        soundfont: None,
    };
    let job = sing.begin();
    let (warnings, _) = sing.run(job, &req, std::slice::from_ref(&singer), &|_| {}).unwrap().unwrap();
    assert!(warnings.iter().any(|x| x.contains("non applicato")), "{warnings:?}");
    let info = sing.track_info(part).unwrap();
    assert_eq!(info.words.iter().find(|x| x.id == w.id).unwrap().phones.iter().map(|p| p.symbol.clone()).collect::<Vec<_>>(), symbols);
    std::fs::remove_dir_all(cache).ok();
}

/// Every word of a part: the phonemes it is sung with, written back as a hint (what the editor
/// saves), give the same phonemes; and the words table of the editor (hints by word) works.
#[test]
fn shown_phonemes_round_trip() {
    let Some(vb) = std::env::var_os("MUSESING_TEST_VOICEBANK").map(PathBuf::from) else {
        eprintln!("MUSESING_TEST_VOICEBANK not set: skipped");
        return;
    };
    use muse_sing_lib::ds::{self, phonemize::{phonemize, split_hint}, voicebank::Voicebank};
    let v = Voicebank::load(&vb).unwrap();
    for (file, lang) in [("agnello.mscz", Lang::It), ("santo.mscz", Lang::It), ("sonnet.mscz", Lang::En)] {
        let score = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(file);
        if !score.exists() {
            eprintln!("fixture missing: skipped");
            return;
        }
        let sc = muse_sing_lib::score::read_mscz(&score).unwrap();
        let build = |hints: &std::collections::HashMap<String, String>| {
            muse_sing_lib::ustx::build(
                &sc,
                &muse_sing_lib::ustx::Options { lang, singer: "", part_singers: None, parts: None, hints, fallback_name: "x" },
            )
        };
        let none = std::collections::HashMap::new();
        let project = build(&none);
        let axis = ds::project_axis(&project);
        let (name, notes) = ds::project_notes(&project).remove(0);
        let speaker = v.speaker_suffix(None);
        let ph = phonemize(&v, lang, &notes, &axis, &speaker).unwrap();
        let per_group = |ph: &ds::phonemize::Phonemized| {
            let mut g: Vec<Vec<String>> = vec![vec![]; ph.groups.len()];
            for p in &ph.phones {
                g[p.group].push(p.symbol.clone());
            }
            g
        };
        let before = per_group(&ph);
        let mut hinted = notes.clone();
        for (gi, g) in ph.groups.iter().enumerate() {
            let n = &mut hinted[g[0]];
            if !before[gi].is_empty() && !n.lyric.starts_with('+') {
                n.lyric = format!("{}[{}]", split_hint(&n.lyric).0, before[gi].join(" "));
            }
        }
        let after = per_group(&phonemize(&v, lang, &hinted, &axis, &speaker).unwrap());
        let diff: Vec<String> = before
            .iter()
            .zip(&after)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| format!("{}: {a:?} -> {b:?}", notes[ph.groups[i][0]].lyric))
            .collect();
        eprintln!("{file} {name}: {} words, {} differ", before.len(), diff.len());
        assert!(diff.is_empty(), "{file}: {diff:?}");

        // the words table: the first word with phonemes, sung everywhere with its first phoneme doubled
        let (gi, syms) = before.iter().enumerate().find(|(i, s)| s.len() >= 2 && !notes[ph.groups[*i][0]].lyric.starts_with('+')).unwrap();
        let word = split_hint(&notes[ph.groups[gi][0]].lyric).0;
        let mut table = syms.clone();
        table.insert(0, syms[0].clone());
        let hints = std::collections::HashMap::from([(word.clone(), table.join(" "))]);
        let (_, n2) = ds::project_notes(&build(&hints)).remove(0);
        let got = per_group(&phonemize(&v, lang, &n2, &axis, &speaker).unwrap());
        let same_word: Vec<usize> = (0..before.len()).filter(|&i| split_hint(&notes[ph.groups[i][0]].lyric).0 == word).collect();
        eprintln!("«{word}» x{}: {:?}", same_word.len(), got[gi]);
        for i in same_word {
            assert_eq!(got[i], table, "«{word}» at group {i}");
        }
    }
}

/// Latin is sung through phonetic hints in Italian G2p symbols: with a real voicebank no hint
/// symbol may be dropped, and every syllable must get exactly one vowel (one per note).
#[test]
fn latin_words_with_a_real_voicebank() {
    use muse_sing_lib::ds::{phonemize, time::TimeAxis, voicebank::Voicebank};
    use muse_sing_lib::lyrics::word_lyrics;
    use muse_sing_lib::score::{Lyric, Note, Syllabic};
    let Some(vb) = std::env::var_os("MUSESING_TEST_VOICEBANK").map(PathBuf::from) else {
        eprintln!("MUSESING_TEST_VOICEBANK not set: skipped");
        return;
    };
    let text = "Glo-ri-a in ex-cel-sis De-o A-gnus De-i qui tol-lis pec-ca-ta mun-di gra-ti-a e-ti-am hos-ti-a \
                Ie-su san-gui-nis mi-hi ni-hil lau-da-mus cae-li ec-ce de-scen-dit sus-ci-pe ex-au-di e-le-i-son \
                Ky-ri-e Chri-ste pa-cem ho-san-na Sa-ba-oth re-gi-na cui huic al-le-lu-ia e-ius Is-ra-el Mi-cha-el \
                e-quus fi-li-i ter-ra in-fer-ni Ze-cha-ri-as quo-ni-am ius-ti-ti-a lin-gua in-ae-sti-ma-bi-lis \
                an-ge-lo-rum reg-gi-a ex-sul-tet";
    let mut notes = vec![];
    for w in text.split_whitespace() {
        let syls: Vec<&str> = w.split('-').collect();
        for (k, s) in syls.iter().enumerate() {
            let syllabic = match (k, syls.len()) {
                (_, 1) => Syllabic::Single,
                (0, _) => Syllabic::Begin,
                (k, n) if k == n - 1 => Syllabic::End,
                _ => Syllabic::Middle,
            };
            notes.push(Note { id: String::new(), start: 0.into(), dur: 1.into(), pitch: 67, lyric: Some(Lyric { text: s.to_string(), syllabic }) });
        }
    }
    let lyrics = word_lyrics(&notes, Lang::La, &Default::default());
    // one word per phrase (a rest between words), one syllable per quarter note
    let mut pnotes = vec![];
    let mut tick = 0;
    for (i, l) in lyrics.iter().enumerate() {
        if i > 0 && !l.starts_with('+') {
            tick += 480;
        }
        pnotes.push(phonemize::PNote { id: i.to_string(), position: tick, duration: 480, tone: 67, lyric: l.clone() });
        tick += 480;
    }
    let vb = Voicebank::load(&vb).expect("voicebank");
    let axis = TimeAxis::new(&[(0, 100.0)], 480);
    let ph = phonemize::phonemize(&vb, Lang::La, &pnotes, &axis, "").expect("phonemize");
    assert!(ph.warnings.is_empty(), "{:?}", ph.warnings);
    let vowels: std::collections::HashSet<String> =
        phonemize::singer_phonemes(&vb, Lang::La).unwrap().into_iter().filter(|(_, v)| *v).map(|(s, _)| s).collect();
    for (g, group) in ph.groups.iter().enumerate() {
        let n = ph.phones.iter().filter(|p| p.group == g && vowels.contains(&p.symbol)).count();
        assert_eq!(n, group.len(), "{}: {:?}", pnotes[group[0]].lyric, ph.phones.iter().filter(|p| p.group == g).map(|p| &p.symbol).collect::<Vec<_>>());
    }
}

/// «Canta e controlla» on a real voice: Tiger singing exactly on the notes is judged in tune, the
/// same raised by 40 cents «quasi».
#[test]
fn checks_a_rendered_voice() {
    use muse_sing_lib::check::{compare, Verdict};
    let Some(vb) = std::env::var_os("MUSESING_TEST_VOICEBANK").map(PathBuf::from) else {
        eprintln!("MUSESING_TEST_VOICEBANK not set: skipped");
        return;
    };
    let score = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agnello.mscz");
    if !score.exists() {
        eprintln!("fixture missing: skipped");
        return;
    }
    let singer = SingerInfo {
        id: "test".into(),
        name: "test".into(),
        singer_type: "diffsinger".into(),
        langs: vec![],
        path: vb.to_string_lossy().into_owned(),
        openutau: false,
        engine: true,
        engine_note: None,
        colors: vec![],
    };
    let cache = std::env::temp_dir().join(format!("muse-sing-check-{}", std::process::id()));
    let sing = Sing::new(Some(cache.clone()));
    let mut req = RenderRequest {
        path: score.to_string_lossy().into_owned(),
        lang: Lang::It,
        pitch: "notes".into(),
        parts: vec![PartReq { name: "Soprano".into(), singer: "test".into(), color: None, voices: 1, played: false }],
        tempo_factor: 1.0,
        accompaniment: false,
        edits: Default::default(),
        instruments: Default::default(),
        marks: true,
        transpose: 0,
        cache_only: false,
        soundfont: None,
    };
    for (cents, expected) in [(0.0f32, Verdict::InTune), (40.0, Verdict::Almost)] {
        if cents != 0.0 {
            let e = muse_sing_lib::edits::PartEdits { pitch: muse_sing_lib::edits::Curve((0..40_000).map(|k| (k * 5, cents)).collect()), ..Default::default() };
            req.edits.insert("Soprano".into(), e);
        }
        let job = sing.begin();
        sing.run(job, &req, std::slice::from_ref(&singer), &|_| {}).unwrap().expect("not cancelled");
        let voice = sing.with_tracks(|t| t.iter().find(|(n, _)| n == "Soprano").unwrap().1.to_vec()).unwrap();
        let track = muse_sing_lib::pitch::analyze(&voice, muse_sing_lib::ds::render::SAMPLE_RATE);
        let notes = sing.melody("Soprano").unwrap();
        let got = compare(&notes, &track, 0.0, (0.0, 1e9));
        let judged: Vec<_> = got.iter().filter(|c| c.verdict != Verdict::NotJudged).collect();
        let hits = judged.iter().filter(|c| c.verdict == expected).count();
        let mut cs: Vec<f32> = judged.iter().filter_map(|c| c.cents).collect();
        cs.sort_by(|a, b| a.total_cmp(b));
        let median = cs[cs.len() / 2];
        let missed = judged.iter().filter(|c| c.verdict == Verdict::Missed).count();
        eprintln!("+{cents} c: {hits} of {} judged {expected:?}, {missed} missed, median {median:.1} c", judged.len());
        assert!(judged.len() * 10 >= got.len() * 8, "most notes judged");
        assert!(hits * 100 >= judged.len() * 75, "+{cents} c: {hits} of {}", judged.len());
        assert!((median - cents).abs() < 12.0, "+{cents} c: median {median}");
        assert!(missed * 20 <= judged.len(), "a voice everywhere: {missed} missed");
    }
    std::fs::remove_dir_all(cache).ok();
}
