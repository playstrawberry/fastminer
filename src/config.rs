#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub ores: bool,
    pub logs: bool,
    pub max_blocks: usize,
    pub breaks_per_tick: usize,
    pub probes_per_tick: usize,
    pub max_jobs: usize,
    pub worlds: Vec<String>,
    pub excluded_tool_data: Vec<(String, String)>,
}

pub const DEFAULT: &str = "enabled=true\nores=true\nlogs=true\nmax_blocks=64\nbreaks_per_tick=4\nprobes_per_tick=104\nmax_jobs=16\nworlds=\nexcluded_tool_data=\n";

const MAX_LIST: usize = 16;
const MAX_ITEM: usize = 64;

/// A comma-separated list: an empty value is an empty list; an empty item, a repeat or an
/// oversized list is refused.
fn list(value: &str) -> Option<Vec<&str>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    let items: Vec<&str> = value.split(',').map(str::trim).collect();
    let unique: std::collections::HashSet<&str> = items.iter().copied().collect();
    (items.len() <= MAX_LIST
        && unique.len() == items.len()
        && items.iter().all(|i| !i.is_empty() && i.len() <= MAX_ITEM))
    .then_some(items)
}

/// `namespace:key`, in the characters item data keys are made of.
fn data_key(item: &str) -> Option<(String, String)> {
    let (namespace, key) = item.split_once(':')?;
    let name = |s: &str, extra: &[char]| {
        !s.is_empty()
            && s.chars().all(|c| {
                c.is_ascii_lowercase()
                    || c.is_ascii_digit()
                    || "_-.".contains(c)
                    || extra.contains(&c)
            })
    };
    (name(namespace, &[]) && name(key, &['/'])).then(|| (namespace.to_owned(), key.to_owned()))
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, usize> {
        if text.len() > 4096 {
            return Err(0);
        }
        let mut c = Self {
            enabled: true,
            ores: true,
            logs: true,
            max_blocks: 64,
            breaks_per_tick: 4,
            probes_per_tick: 104,
            max_jobs: 16,
            worlds: Vec::new(),
            excluded_tool_data: Vec::new(),
        };
        let mut keys = std::collections::HashSet::new();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let err = || i + 1;
            let (key, value) = line.split_once('=').ok_or_else(err)?;
            let (key, value) = (key.trim(), value.trim());
            if !keys.insert(key) {
                return Err(err());
            }
            match key {
                "enabled" => c.enabled = value.parse().map_err(|_| err())?,
                "ores" => c.ores = value.parse().map_err(|_| err())?,
                "logs" => c.logs = value.parse().map_err(|_| err())?,
                "worlds" => {
                    c.worlds = list(value)
                        .ok_or_else(err)?
                        .into_iter()
                        .map(str::to_owned)
                        .collect();
                }
                "excluded_tool_data" => {
                    c.excluded_tool_data = list(value)
                        .ok_or_else(err)?
                        .into_iter()
                        .map(data_key)
                        .collect::<Option<_>>()
                        .ok_or_else(err)?;
                }
                _ => {
                    let n: usize = value.parse().map_err(|_| err())?;
                    let (field, max) = match key {
                        "max_blocks" => (&mut c.max_blocks, 256),
                        "breaks_per_tick" => (&mut c.breaks_per_tick, 8),
                        "probes_per_tick" => (&mut c.probes_per_tick, 208),
                        "max_jobs" => (&mut c.max_jobs, 16),
                        _ => return Err(err()),
                    };
                    if n == 0 || n > max {
                        return Err(err());
                    }
                    *field = n;
                }
            }
        }
        Ok(c)
    }

    pub fn allows_world(&self, world: &str) -> bool {
        self.worlds.is_empty() || self.worlds.iter().any(|w| w == world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_bounded_and_configurable() {
        let c = Config::parse(DEFAULT).unwrap();
        assert_eq!(c.max_blocks, 64);
        assert_eq!(c.breaks_per_tick, 4);
        assert_eq!(c.probes_per_tick, 104);
        assert_eq!(c.max_jobs, 16);
        assert!(c.enabled && c.ores && c.logs);
        assert_eq!(
            Config::parse("enabled=false\nmax_blocks=1")
                .unwrap()
                .max_blocks,
            1
        );
    }

    #[test]
    fn empty_world_list_allows_every_world_and_a_list_allows_only_its_worlds() {
        let all = Config::parse(DEFAULT).unwrap();
        assert!(all.worlds.is_empty() && all.allows_world("anything"));
        let some = Config::parse("worlds=mining, mining_nether").unwrap();
        assert_eq!(some.worlds, ["mining", "mining_nether"]);
        assert!(some.allows_world("mining") && some.allows_world("mining_nether"));
        assert!(!some.allows_world("spawn") && !some.allows_world("MINING"));
    }

    #[test]
    fn excluded_tool_data_is_a_list_of_exact_namespaced_keys() {
        assert!(
            Config::parse(DEFAULT)
                .unwrap()
                .excluded_tool_data
                .is_empty()
        );
        let c = Config::parse("excluded_tool_data=shop:drill, my_plugin:tool.id-2/x").unwrap();
        assert_eq!(
            c.excluded_tool_data,
            [
                ("shop".to_owned(), "drill".to_owned()),
                ("my_plugin".to_owned(), "tool.id-2/x".to_owned())
            ]
        );
    }

    #[test]
    fn malformed_or_unbounded_lists_are_refused() {
        let many = |key: &str, item: &str| {
            let items: Vec<_> = (0..17).map(|n| format!("{item}{n}")).collect();
            format!("{key}={}", items.join(","))
        };
        for text in [
            "worlds=a,,b".to_owned(),
            "worlds=a,a".to_owned(),
            format!("worlds={}", "w".repeat(65)),
            many("worlds", "w"),
            "excluded_tool_data=nokey".to_owned(),
            "excluded_tool_data=a:b:c".to_owned(),
            "excluded_tool_data=:b".to_owned(),
            "excluded_tool_data=a:".to_owned(),
            "excluded_tool_data=A:b".to_owned(),
            "excluded_tool_data=a:b,a:b".to_owned(),
            many("excluded_tool_data", "a:b"),
        ] {
            assert!(Config::parse(&text).is_err(), "{text}");
        }
    }

    #[test]
    fn malformed_duplicate_unknown_and_oversized_config_is_refused() {
        for text in [
            "max_blocks=0",
            "max_blocks=257",
            "breaks_per_tick=0",
            "breaks_per_tick=9",
            "probes_per_tick=0",
            "probes_per_tick=209",
            "max_jobs=17",
            "logs=yes",
            "enabled=true\nenabled=false",
            "typo=4",
            "enabled",
            "max_blocks=-1",
        ] {
            assert!(Config::parse(text).is_err(), "{text}");
        }
        assert!(Config::parse(&" ".repeat(4097)).is_err());
    }
}
