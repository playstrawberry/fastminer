use strawberry_plugin_api::wit::strawberry::plugin::{
    data_components::DataComponent, item_stack::DataComponentValue,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Ore(u8),
    Log,
}

fn vanilla(key: &str) -> Option<&str> {
    let key = key.strip_prefix("minecraft:").unwrap_or(key);
    (!key.contains(':')).then_some(key)
}

pub fn kind(block: &str) -> Option<Kind> {
    let block = vanilla(block)?;
    let ore = block.strip_prefix("deepslate_").unwrap_or(block);
    let tier = match ore {
        "coal_ore" | "nether_gold_ore" | "nether_quartz_ore" => Some(0),
        "copper_ore" | "iron_ore" | "lapis_ore" => Some(1),
        "gold_ore" | "redstone_ore" | "diamond_ore" | "emerald_ore" => Some(2),
        "ancient_debris" => Some(3),
        _ => None,
    };
    if let Some(tier) = tier {
        return Some(Kind::Ore(tier));
    }
    let wood = block.strip_prefix("stripped_").unwrap_or(block);
    let species = wood
        .strip_suffix("_log")
        .or_else(|| wood.strip_suffix("_wood"));
    if species.is_some_and(|s| WOODS.contains(&s))
        || matches!(
            wood,
            "crimson_stem" | "warped_stem" | "crimson_hyphae" | "warped_hyphae"
        )
    {
        Some(Kind::Log)
    } else {
        None
    }
}

const WOODS: &[&str] = &[
    "oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry", "pale_oak",
];

pub fn blocks() -> Vec<String> {
    let mut names = Vec::new();
    for ore in [
        "coal", "copper", "iron", "lapis", "gold", "redstone", "diamond", "emerald",
    ] {
        names.push(format!("{ore}_ore"));
        names.push(format!("deepslate_{ore}_ore"));
    }
    names.extend(["nether_gold_ore", "nether_quartz_ore", "ancient_debris"].map(str::to_owned));
    for wood in WOODS {
        for form in ["log", "wood"] {
            names.push(format!("{wood}_{form}"));
            names.push(format!("stripped_{wood}_{form}"));
        }
    }
    for wood in ["crimson", "warped"] {
        for form in ["stem", "hyphae"] {
            names.push(format!("{wood}_{form}"));
            names.push(format!("stripped_{wood}_{form}"));
        }
    }
    names
}

/// The item and its data, compared while a vein runs. Wear from the vein itself is left out.
#[derive(Debug, PartialEq, Eq)]
pub struct ToolIdentity {
    key: String,
    data: Vec<(u32, Vec<u8>)>,
}

impl ToolIdentity {
    pub fn new(key: String, components: Vec<DataComponentValue>) -> Self {
        let mut data: Vec<_> = components
            .into_iter()
            .filter(|c| c.component != DataComponent::Damage)
            .map(|c| (c.component as u32, c.value))
            .collect();
        data.sort_unstable();
        Self { key, data }
    }
}

pub fn correct_tool(kind: Kind, tool: &str) -> bool {
    let Some(tool) = vanilla(tool) else {
        return false;
    };
    let material = match kind {
        Kind::Log => tool.strip_suffix("_axe"),
        Kind::Ore(_) => tool.strip_suffix("_pickaxe"),
    };
    let tier = match material {
        Some("wooden" | "golden") => 0,
        Some("stone" | "copper") => 1,
        Some("iron") => 2,
        Some("diamond" | "netherite") => 3,
        _ => return false,
    };
    match kind {
        Kind::Ore(required) => tier >= required,
        Kind::Log => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(component: DataComponent, value: &[u8]) -> DataComponentValue {
        DataComponentValue {
            component,
            value: value.to_vec(),
        }
    }
    fn identity(key: &str, components: Vec<DataComponentValue>) -> ToolIdentity {
        ToolIdentity::new(key.to_owned(), components)
    }

    #[test]
    fn wear_and_component_order_do_not_change_a_tools_identity() {
        let fresh = identity(
            "minecraft:iron_pickaxe",
            vec![
                value(DataComponent::CustomData, &[1]),
                value(DataComponent::Enchantments, &[2]),
            ],
        );
        let worn = identity(
            "minecraft:iron_pickaxe",
            vec![
                value(DataComponent::Enchantments, &[2]),
                value(DataComponent::Damage, &[9]),
                value(DataComponent::CustomData, &[1]),
            ],
        );
        assert_eq!(fresh, worn);
    }

    #[test]
    fn another_item_or_other_data_is_another_tool() {
        let base = identity(
            "minecraft:iron_pickaxe",
            vec![value(DataComponent::Enchantments, &[2])],
        );
        assert_ne!(
            base,
            identity(
                "minecraft:diamond_pickaxe",
                vec![value(DataComponent::Enchantments, &[2])],
            )
        );
        assert_ne!(
            base,
            identity(
                "minecraft:iron_pickaxe",
                vec![value(DataComponent::Enchantments, &[3])],
            )
        );
        assert_ne!(base, identity("minecraft:iron_pickaxe", vec![]));
        assert_ne!(
            base,
            identity(
                "minecraft:iron_pickaxe",
                vec![
                    value(DataComponent::Enchantments, &[2]),
                    value(DataComponent::CustomData, &[1]),
                ],
            )
        );
    }

    #[test]
    fn ore_tiers_and_tool_classes() {
        for (block, tool, expected) in [
            ("coal_ore", "wooden_pickaxe", true),
            ("copper_ore", "wooden_pickaxe", false),
            ("copper_ore", "copper_pickaxe", true),
            ("deepslate_iron_ore", "stone_pickaxe", true),
            ("diamond_ore", "golden_pickaxe", false),
            ("diamond_ore", "iron_pickaxe", true),
            ("ancient_debris", "iron_pickaxe", false),
            ("ancient_debris", "diamond_pickaxe", true),
            ("nether_quartz_ore", "wooden_pickaxe", true),
            ("oak_log", "wooden_axe", true),
            ("stripped_pale_oak_log", "copper_axe", true),
            ("warped_stem", "netherite_axe", true),
            ("oak_log", "diamond_pickaxe", false),
            ("iron_ore", "diamond_axe", false),
            ("oak_log", "custom:diamond_axe", false),
        ] {
            assert_eq!(
                correct_tool(kind(block).expect(block), tool),
                expected,
                "{block} {tool}"
            );
        }
        assert_eq!(kind("minecraft:oak_log"), kind("oak_log"));
        for block in [
            "custom:oak_log",
            "stone",
            "oak_leaves",
            "diamond_block",
            "fake_log",
        ] {
            assert_eq!(kind(block), None, "{block}");
        }
    }
}
