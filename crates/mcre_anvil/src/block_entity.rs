use mcre_world::BlockEntity;

use crate::nbt::Tag;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockEntityPayload {
    Sign(SignData),
    Inventory(Vec<InventoryItem>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignData {
    pub front: SignText,
    pub back: SignText,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignText {
    pub lines: Vec<String>,
    pub color: String,
    pub glowing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryItem {
    pub slot: u8,
    pub id: String,
    pub count: u32,
}

pub fn decode_block_entity_payload(entity: &BlockEntity) -> Option<BlockEntityPayload> {
    let (_, root) = crate::nbt::read_named(entity.nbt()).ok()?;
    match entity.id() {
        "minecraft:sign" | "minecraft:hanging_sign" => {
            Some(BlockEntityPayload::Sign(decode_sign_data(&root)))
        }
        _ => decode_inventory(&root).map(BlockEntityPayload::Inventory),
    }
}

fn decode_sign_data(root: &Tag) -> SignData {
    let front = if root.get("front_text").is_some() {
        decode_sign_text(root.get("front_text"))
    } else {
        decode_legacy_sign_text(root)
    };
    SignData {
        front,
        back: decode_sign_text(root.get("back_text")),
    }
}

fn decode_legacy_sign_text(root: &Tag) -> SignText {
    let lines = (1..=4)
        .map(|index| {
            root.get(&format!("Text{index}"))
                .map(component_text)
                .unwrap_or_default()
        })
        .collect();
    SignText {
        lines,
        color: "black".into(),
        glowing: false,
    }
}

fn decode_sign_text(tag: Option<&Tag>) -> SignText {
    let mut lines: Vec<String> = tag
        .and_then(|tag| tag.get("messages"))
        .and_then(Tag::as_list)
        .map(|messages| messages.iter().take(4).map(component_text).collect())
        .unwrap_or_default();
    lines.resize(4, String::new());
    let color = tag
        .and_then(|tag| tag.get("color"))
        .and_then(Tag::as_str)
        .unwrap_or("black")
        .to_string();
    let glowing = tag
        .and_then(|tag| tag.get("has_glowing_text"))
        .and_then(Tag::as_i32)
        .is_some_and(|value| value != 0);
    SignText {
        lines,
        color,
        glowing,
    }
}

fn component_text(tag: &Tag) -> String {
    match tag {
        Tag::String(value) => serde_json::from_str(value)
            .map(|value| json_component_text(&value))
            .unwrap_or_else(|_| value.clone()),
        Tag::Compound(_) => {
            let mut text = tag
                .get("text")
                .or_else(|| tag.get("translate"))
                .or_else(|| tag.get("selector"))
                .or_else(|| tag.get("keybind"))
                .and_then(Tag::as_str)
                .unwrap_or_default()
                .to_string();
            if let Some(extra) = tag.get("extra").and_then(Tag::as_list) {
                for component in extra {
                    text.push_str(&component_text(component));
                }
            }
            text
        }
        Tag::List(components) => components.iter().map(component_text).collect(),
        _ => String::new(),
    }
}

fn json_component_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(components) => {
            components.iter().map(json_component_text).collect()
        }
        serde_json::Value::Object(component) => {
            let mut text = component
                .get("text")
                .or_else(|| component.get("translate"))
                .or_else(|| component.get("selector"))
                .or_else(|| component.get("keybind"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            if let Some(score) = component
                .get("score")
                .and_then(|score| score.get("value"))
                .and_then(serde_json::Value::as_str)
            {
                text.push_str(score);
            }
            if let Some(extra) = component.get("extra").and_then(serde_json::Value::as_array) {
                for component in extra {
                    text.push_str(&json_component_text(component));
                }
            }
            text
        }
        _ => String::new(),
    }
}

fn decode_inventory(root: &Tag) -> Option<Vec<InventoryItem>> {
    let items = root.get("Items").or_else(|| root.get("items"))?.as_list()?;
    Some(
        items
            .iter()
            .filter_map(|item| {
                let slot = item
                    .get("Slot")
                    .or_else(|| item.get("slot"))
                    .and_then(Tag::as_i32)
                    .and_then(|slot| u8::try_from(slot).ok())?;
                let id = item.get("id").and_then(Tag::as_str)?.to_string();
                let count = item
                    .get("count")
                    .or_else(|| item.get("Count"))
                    .and_then(Tag::as_i32)
                    .filter(|count| *count > 0)? as u32;
                Some(InventoryItem { slot, id, count })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_entity(id: &str, payload: Tag) -> BlockEntity {
        let mut payload = payload;
        payload.set("id", Tag::String(id.into()));
        BlockEntity::new(id.into(), (1, 64, 2), crate::nbt::write_named("", &payload))
    }

    #[test]
    fn decodes_both_sign_sides_and_component_json() {
        let sign = block_entity(
            "minecraft:sign",
            Tag::Compound(vec![
                (
                    "front_text".into(),
                    Tag::Compound(vec![
                        (
                            "messages".into(),
                            Tag::List(vec![
                                Tag::String("Hello".into()),
                                Tag::String(
                                    r#"{"text":"Bright ","extra":[{"text":"sign"}]}"#.into(),
                                ),
                            ]),
                        ),
                        ("color".into(), Tag::String("blue".into())),
                        ("has_glowing_text".into(), Tag::Byte(1)),
                    ]),
                ),
                (
                    "back_text".into(),
                    Tag::Compound(vec![(
                        "messages".into(),
                        Tag::List(vec![Tag::String("Back".into())]),
                    )]),
                ),
            ]),
        );

        let Some(BlockEntityPayload::Sign(data)) = decode_block_entity_payload(&sign) else {
            panic!("expected sign payload");
        };
        assert_eq!(data.front.lines, ["Hello", "Bright sign", "", ""]);
        assert_eq!(data.front.color, "blue");
        assert!(data.front.glowing);
        assert_eq!(data.back.lines, ["Back", "", "", ""]);
        assert_eq!(data.back.color, "black");
        assert!(!data.back.glowing);
        assert_eq!(
            component_text(&Tag::Compound(vec![
                ("text".into(), Tag::String("NBT ".into())),
                (
                    "extra".into(),
                    Tag::List(vec![Tag::String("component".into())]),
                ),
            ])),
            "NBT component"
        );
    }

    #[test]
    fn decodes_legacy_sign_lines() {
        let sign = block_entity(
            "minecraft:sign",
            Tag::Compound(vec![
                ("Text1".into(), Tag::String(r#"{"text":"Old"}"#.into())),
                ("Text2".into(), Tag::String("format".into())),
            ]),
        );

        let Some(BlockEntityPayload::Sign(data)) = decode_block_entity_payload(&sign) else {
            panic!("expected sign payload");
        };
        assert_eq!(data.front.lines, ["Old", "format", "", ""]);
        assert_eq!(data.back.lines, ["", "", "", ""]);
    }

    #[test]
    fn decodes_modern_and_legacy_inventory_entries() {
        let chest = block_entity(
            "minecraft:chest",
            Tag::Compound(vec![(
                "Items".into(),
                Tag::List(vec![
                    Tag::Compound(vec![
                        ("id".into(), Tag::String("minecraft:diamond".into())),
                        ("count".into(), Tag::Int(12)),
                        ("Slot".into(), Tag::Byte(3)),
                    ]),
                    Tag::Compound(vec![
                        ("id".into(), Tag::String("minecraft:apple".into())),
                        ("Count".into(), Tag::Byte(4)),
                        ("Slot".into(), Tag::Byte(7)),
                    ]),
                    Tag::Compound(vec![
                        ("id".into(), Tag::String("minecraft:air".into())),
                        ("count".into(), Tag::Int(0)),
                        ("Slot".into(), Tag::Byte(8)),
                    ]),
                ]),
            )]),
        );

        assert_eq!(
            decode_block_entity_payload(&chest),
            Some(BlockEntityPayload::Inventory(vec![
                InventoryItem {
                    slot: 3,
                    id: "minecraft:diamond".into(),
                    count: 12,
                },
                InventoryItem {
                    slot: 7,
                    id: "minecraft:apple".into(),
                    count: 4,
                },
            ]))
        );
    }
}
