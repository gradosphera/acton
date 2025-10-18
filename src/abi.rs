use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::Mutex;
use tree_sitter::Node;

#[derive(Debug, Clone)]
pub struct FieldDescription {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone)]
pub struct StructDescription {
    pub fields: Vec<FieldDescription>,
}

lazy_static! {
    static ref STRUCT_DEFINITIONS: Mutex<HashMap<String, StructDescription>> =
        Mutex::new(HashMap::new());
}

pub fn get_struct_field_names(type_name: &str) -> Option<Vec<String>> {
    STRUCT_DEFINITIONS
        .lock()
        .unwrap()
        .get(type_name)
        .map(|desc| desc.fields.iter().map(|f| f.name.clone()).collect())
}

pub fn get_struct_field_types(type_name: &str) -> Option<Vec<String>> {
    STRUCT_DEFINITIONS
        .lock()
        .unwrap()
        .get(type_name)
        .map(|desc| desc.fields.iter().map(|f| f.type_name.clone()).collect())
}

pub fn process_struct_definitions(node: &Node, content: &str, file_path: &str) {
    let mut struct_defs = HashMap::new();
    analyze_structs_recursive(&node, content, file_path, &mut struct_defs);
    *STRUCT_DEFINITIONS.lock().unwrap() = struct_defs;
    emulator::tuple::stack::set_struct_field_getter(get_struct_field_names);
    emulator::tuple::stack::set_struct_field_type_getter(get_struct_field_types);
}

fn analyze_structs_recursive(
    node: &Node,
    content: &str,
    file_path: &str,
    struct_defs: &mut HashMap<String, StructDescription>,
) {
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        analyze_structs_recursive(&child, content, file_path, struct_defs);
    }

    if node.kind() != "struct_declaration" {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };

    let struct_name = name_node
        .utf8_text(content.as_bytes())
        .unwrap_or("")
        .to_string();

    let mut fields = Vec::new();

    let Some(body_node) = node.child_by_field_name("body") else {
        return;
    };

    let mut cursor = body_node.walk();
    for child in body_node.children(&mut cursor) {
        if child.kind() == "struct_field_declaration" {
            let Some(field_name_node) = child.child_by_field_name("name") else {
                continue;
            };

            let Some(field_type_node) = child.child_by_field_name("type") else {
                continue;
            };

            let field_name = field_name_node
                .utf8_text(content.as_bytes())
                .unwrap_or("")
                .to_string();

            let field_type = field_type_node
                .utf8_text(content.as_bytes())
                .unwrap_or("")
                .to_string();

            fields.push(FieldDescription {
                name: field_name,
                type_name: field_type,
            });
        }
    }

    if !fields.is_empty() {
        struct_defs.insert(struct_name, StructDescription { fields });
    }
}
