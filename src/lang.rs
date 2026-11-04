const FILES: [(&str, &str); 4] = [
    ("en", include_str!("../lang/en.lang")),
    ("fr", include_str!("../lang/fr.lang")),
    ("es", include_str!("../lang/es.lang")),
    ("tr", include_str!("../lang/tr.lang")),
];

pub fn text(locale: &str, key: &str, args: &[&str]) -> String {
    let code = locale.get(..2).unwrap_or("en").to_ascii_lowercase();
    let file = FILES
        .iter()
        .find(|(c, _)| *c == code)
        .unwrap_or(&FILES[0])
        .1;
    let lookup = |file: &str| {
        file.lines()
            .filter_map(|l| l.split_once('='))
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.to_owned())
    };
    let mut out = lookup(file)
        .or_else(|| lookup(FILES[0].1))
        .unwrap_or_else(|| key.to_owned());
    for (i, arg) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), arg);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalogs_match_keys_and_placeholders() {
        let rows = |s: &str| {
            s.lines()
                .filter_map(|l| l.split_once('='))
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect::<std::collections::BTreeMap<_, _>>()
        };
        let en = rows(FILES[0].1);
        for (_, file) in FILES {
            let current = rows(file);
            assert_eq!(
                current.keys().collect::<Vec<_>>(),
                en.keys().collect::<Vec<_>>()
            );
            for (key, value) in &current {
                for p in ["{0}", "{1}", "{2}"] {
                    assert_eq!(value.contains(p), en[key].contains(p), "{key} {p}");
                }
            }
        }
        assert_eq!(text("💚", "usage", &[]), text("en", "usage", &[]));
    }
}
