use std::collections::HashMap;

const CRC16: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::CRC_16_XMODEM);

#[derive(Debug, Clone)]
pub struct Pos {
    pub row: usize,
    pub column: usize,
    pub uri: String,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub type_info: TypeInfo,
}

#[derive(Debug, Clone)]
pub enum BaseTypeInfo {
    Void,
    Int { width: usize },
    UInt { width: usize },
    Coins,
    Bool,
    Address,
    Bits { width: usize },
    Cell { inner_type: Option<Box<TypeInfo>> },
    Slice,
    VarInt16,
    VarInt32,
    VarUInt16,
    VarUInt32,
    Struct { struct_name: String },
    AnonStruct { fields: Vec<TypeInfo> },
}

#[derive(Debug, Clone)]
pub struct TypeInfo {
    pub base: BaseTypeInfo,
    pub human_readable: String,
}

#[derive(Debug, Clone)]
pub struct TypeAbi {
    pub name: String,
    pub opcode: Option<u32>,
    pub opcode_width: Option<usize>,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone)]
pub struct GetMethod {
    pub name: String,
    pub id: u32,
    pub pos: Option<Pos>,
    pub return_type: TypeInfo,
    pub parameters: Vec<Field>,
}

#[derive(Debug, Clone)]
pub struct ExitCodeInfo {
    pub constant_name: String,
    pub value: i32,
    pub usage_positions: Vec<Pos>,
}

#[derive(Debug, Clone)]
pub struct EntryPoint {
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone)]
pub struct ContractAbi {
    pub name: String,
    pub entry_point: Option<EntryPoint>,
    pub external_entry_point: Option<EntryPoint>,
    pub storage: Option<TypeAbi>,
    pub get_methods: Vec<GetMethod>,
    pub messages: Vec<TypeAbi>,
    pub types: Vec<TypeAbi>,
}

#[derive(Debug)]
struct AbiInfo {
    get_methods: Vec<GetMethod>,
    messages: Vec<TypeAbi>,
    types: Vec<TypeAbi>,
    storage: Option<TypeAbi>,
    entry_point: Option<EntryPoint>,
    external_entry_point: Option<EntryPoint>,
}

#[derive(Debug, Clone)]
pub struct ABI {
    pub structs: HashMap<String, StructDescription>,
}

impl ABI {
    pub fn find_type(&self, name: &String) -> Option<StructDescription> {
        self.structs.get(name).cloned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDescription {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone, Eq)]
pub struct StructDescription {
    pub fields: Vec<FieldDescription>,
}

impl PartialEq for StructDescription {
    fn eq(&self, other: &Self) -> bool {
        self.fields.len() == other.fields.len()
            && self.fields.iter().all(|field| other.fields.contains(field))
    }
}

pub fn process_struct_definitions(
    node: &tree_sitter::Node,
    content: &str,
    file_path: &str,
) -> HashMap<String, StructDescription> {
    let mut struct_defs = HashMap::new();
    analyze_structs_recursive(&node, content, file_path, &mut struct_defs);
    struct_defs
}

fn analyze_structs_recursive(
    node: &tree_sitter::Node,
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

pub fn contract_abi(node: &tree_sitter::Node, content: &str, file_path: &str) -> ContractAbi {
    let contract_name = get_contract_name_from_file_path(file_path);

    let abi_info = collect_abi_info(node, content, file_path);

    ContractAbi {
        name: contract_name,
        entry_point: abi_info.entry_point,
        external_entry_point: abi_info.external_entry_point,
        storage: abi_info.storage,
        get_methods: abi_info.get_methods,
        messages: abi_info.messages,
        types: abi_info.types,
    }
}

fn collect_abi_info(node: &tree_sitter::Node, content: &str, file_path: &str) -> AbiInfo {
    let mut info = AbiInfo {
        get_methods: Vec::new(),
        messages: Vec::new(),
        types: Vec::new(),
        storage: None,
        entry_point: None,
        external_entry_point: None,
    };

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_declaration" {
            let Some(name_node) = child.child_by_field_name("name") else {
                continue;
            };

            let func_name = name_node
                .utf8_text(content.as_bytes())
                .unwrap_or("")
                .to_string();

            if func_name == "onInternalMessage" {
                info.entry_point = Some(EntryPoint {
                    pos: Some(Pos {
                        row: name_node.start_position().row,
                        column: name_node.start_position().column,
                        uri: file_path.to_string(),
                    }),
                });
            } else if func_name == "onExternalMessage" {
                info.external_entry_point = Some(EntryPoint {
                    pos: Some(Pos {
                        row: name_node.start_position().row,
                        column: name_node.start_position().column,
                        uri: file_path.to_string(),
                    }),
                });
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "get_method_declaration" {
            let Some(method) = extract_get_method(&child, content, file_path) else {
                continue;
            };
            info.get_methods.push(method);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "struct_declaration" {
            let Some(struct_abi) = extract_struct_abi(&child, content, file_path) else {
                continue;
            };

            if struct_abi.name == "Storage" {
                info.storage = Some(struct_abi.clone());
            }

            if struct_abi.opcode.is_some() {
                info.messages.push(struct_abi.clone());
            }
            info.types.push(struct_abi);
        }
    }

    info
}

fn extract_get_method(
    func_node: &tree_sitter::Node,
    content: &str,
    file_path: &str,
) -> Option<GetMethod> {
    let name_node = func_node.child_by_field_name("name")?;
    let func_name = name_node
        .utf8_text(content.as_bytes())
        .unwrap_or("")
        .to_string();

    let explicit_id = get_explicit_method_id(func_node, content);
    let method_id = match explicit_id {
        Some(id) => id,
        None => (CRC16.checksum(func_name.as_bytes()) as u32 & 0xFFFF) | 0x10000,
    };

    let pos = Pos {
        row: name_node.start_position().row,
        column: name_node.start_position().column,
        uri: file_path.to_string(),
    };

    let parameters = extract_parameters(func_node, content, file_path);

    let return_type = TypeInfo {
        base: BaseTypeInfo::Void,
        human_readable: "void".to_string(),
    };

    Some(GetMethod {
        name: func_name,
        id: method_id,
        pos: Some(pos),
        return_type,
        parameters,
    })
}

fn extract_struct_abi(
    struct_node: &tree_sitter::Node,
    content: &str,
    file_path: &str,
) -> Option<TypeAbi> {
    let name_node = struct_node.child_by_field_name("name")?;
    let struct_name = name_node
        .utf8_text(content.as_bytes())
        .unwrap_or("")
        .to_string();

    let mut fields = Vec::new();

    if let Some(body_node) = struct_node.child_by_field_name("body") {
        let mut cursor = body_node.walk();
        for child in body_node
            .children(&mut cursor)
            .filter(|child| child.kind() == "struct_field_declaration")
        {
            if let Some(field) = extract_field(&child, content, file_path) {
                fields.push(field);
            }
        }
    }

    let mut opcode = None;
    let mut opcode_width = None;

    if let Some(prefix_node) = struct_node.child_by_field_name("pack_prefix") {
        let prefix_text = prefix_node
            .utf8_text(content.as_bytes())
            .unwrap_or("")
            .to_string();

        // Clean the number by removing underscores
        let clean_text = prefix_text.replace('_', "");

        let (prefix_val, radix) = if clean_text.starts_with("0x") {
            (u32::from_str_radix(&clean_text[2..], 16), 16)
        } else if clean_text.starts_with("0b") {
            (u32::from_str_radix(&clean_text[2..], 2), 2)
        } else {
            (clean_text.parse::<u32>(), 10)
        };

        if let Ok(val) = prefix_val {
            opcode = Some(val);
            opcode_width = match radix {
                16 => Some((clean_text.len() - 2) * 4),
                2 => Some(clean_text.len() - 2),
                _ => Some(format!("{:b}", val).len()),
            };
        }
    }

    Some(TypeAbi {
        name: struct_name,
        opcode,
        opcode_width,
        fields,
    })
}

fn extract_field(field_node: &tree_sitter::Node, content: &str, _file_path: &str) -> Option<Field> {
    let name_node = field_node.child_by_field_name("name")?;
    let type_node = field_node.child_by_field_name("type")?;

    let field_name = name_node
        .utf8_text(content.as_bytes())
        .unwrap_or("")
        .to_string();

    let type_name = type_node
        .utf8_text(content.as_bytes())
        .unwrap_or("")
        .to_string();

    let type_info = TypeInfo {
        base: BaseTypeInfo::Void,
        human_readable: type_name.clone(),
    };

    Some(Field {
        name: field_name,
        type_info,
    })
}

fn extract_parameters(func_node: &tree_sitter::Node, content: &str, file_path: &str) -> Vec<Field> {
    let mut parameters = Vec::new();

    let Some(params_node) = func_node.child_by_field_name("parameters") else {
        return parameters;
    };

    let mut cursor = params_node.walk();
    for child in params_node
        .children(&mut cursor)
        .filter(|child| child.kind() == "parameter_declaration")
    {
        if let Some(field) = extract_field(&child, content, file_path) {
            parameters.push(field);
        }
    }

    parameters
}

fn get_explicit_method_id(func_node: &tree_sitter::Node, content: &str) -> Option<u32> {
    let annotations = func_node.child_by_field_name("annotations")?;
    let mut cursor = annotations.walk();

    for child in annotations
        .children(&mut cursor)
        .filter(|child| child.kind() == "annotation")
    {
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };

        let annotation_name = name_node
            .utf8_text(content.as_bytes())
            .unwrap_or("")
            .to_string();

        if annotation_name == "method_id" {
            let Some(args_node) = child.child_by_field_name("arguments") else {
                continue;
            };

            let mut args_cursor = args_node.walk();
            for arg in args_node.children(&mut args_cursor) {
                if arg.kind() == "number_literal" {
                    let value_text = arg.utf8_text(content.as_bytes()).unwrap_or("").to_string();

                    let id = if value_text.starts_with("0x") {
                        u32::from_str_radix(&value_text[2..], 16).ok()
                    } else {
                        value_text.parse::<u32>().ok()
                    };

                    return id;
                }
            }
        }
    }

    None
}

fn get_contract_name_from_file_path(file_path: &str) -> String {
    let file_name = std::path::Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown");

    file_name.split('.').next().unwrap_or("Unknown").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tolk_parser::parser::parse;

    #[test]
    fn test_contract_abi_basic() {
        let code = r#"
struct Storage {
    balance: int;
}

get fun get_balance(): int {
    return 0;
}

fun onInternalMessage() {
}
"#;

        let tree = parse(code).unwrap();
        let root_node = tree.root_node();
        let abi = contract_abi(&root_node, code, "test.tolk");

        assert_eq!(abi.name, "test");
        assert!(abi.entry_point.is_some());
        assert!(abi.storage.is_some());
        assert_eq!(abi.get_methods.len(), 1);
        assert_eq!(abi.get_methods[0].name, "get_balance");

        let expected_id = (CRC16.checksum(b"get_balance") as u32 & 0xFFFF) | 0x10000;
        assert_eq!(abi.get_methods[0].id, expected_id);
    }

    #[test]
    fn test_contract_abi_explicit_method_id() {
        let code = r#"
@method_id(0x12345)
get fun custom_method(): int {
    return 42;
}
"#;

        let tree = parse(code).unwrap();
        let root_node = tree.root_node();
        let abi = contract_abi(&root_node, code, "test.tolk");

        assert_eq!(abi.get_methods.len(), 1);
        assert_eq!(abi.get_methods[0].name, "custom_method");
        assert_eq!(abi.get_methods[0].id, 0x12345);
    }

    #[test]
    fn test_get_method_variants() {
        let code = r#"
// Get method with parameters and return type
get fun get_balance(addr: address): int {
    return 0;
}

// Get method without parameters
get fun get_counter(): int {
    return 42;
}

// Get method without return type annotation
get fun ping() {
    return;
}

// Get method with just 'get' (no 'fun')
get simple_method(): int {
    return 1;
}

// Get method with method_id annotation
@method_id(0x10001)
get fun custom_id(): int {
    return 2;
}
"#;

        let tree = parse(code).unwrap();
        let root_node = tree.root_node();
        let abi = contract_abi(&root_node, code, "test.tolk");

        assert_eq!(abi.get_methods.len(), 5);

        let names: Vec<&str> = abi.get_methods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"get_balance"));
        assert!(names.contains(&"get_counter"));
        assert!(names.contains(&"ping"));
        assert!(names.contains(&"simple_method"));
        assert!(names.contains(&"custom_id"));

        let custom_id_method = abi
            .get_methods
            .iter()
            .find(|m| m.name == "custom_id")
            .unwrap();
        assert_eq!(custom_id_method.id, 0x10001);
    }

    #[test]
    fn test_struct_variants() {
        let code = r#"
// Regular struct
struct User {
    id: int;
    name: string;
}

// Storage struct
struct Storage {
    counter: int;
    owner: address;
}

// Struct with hex pack prefix
struct (0xABCD) MessageData {
    data: cell;
}

// Struct with decimal pack prefix
struct (123) TokenInfo {
    amount: int;
    symbol: string;
}

// Struct with binary pack prefix
struct (0b1010) BinaryData {
    flag: bool;
}
"#;

        let tree = parse(code).unwrap();
        let root_node = tree.root_node();
        let abi = contract_abi(&root_node, code, "test.tolk");

        assert_eq!(abi.types.len(), 5);
        assert_eq!(abi.messages.len(), 3); // Only structs with pack_prefix
        assert!(abi.storage.is_some());

        assert_eq!(abi.storage.as_ref().unwrap().name, "Storage");

        let message_names: Vec<&str> = abi.messages.iter().map(|m| m.name.as_str()).collect();
        assert!(message_names.contains(&"MessageData"));
        assert!(message_names.contains(&"TokenInfo"));
        assert!(message_names.contains(&"BinaryData"));

        let message_data = abi
            .messages
            .iter()
            .find(|m| m.name == "MessageData")
            .unwrap();
        assert_eq!(message_data.opcode, Some(0xABCD));

        let token_info = abi.messages.iter().find(|m| m.name == "TokenInfo").unwrap();
        assert_eq!(token_info.opcode, Some(123));

        let binary_data = abi
            .messages
            .iter()
            .find(|m| m.name == "BinaryData")
            .unwrap();
        assert_eq!(binary_data.opcode, Some(0b1010));
    }

    #[test]
    fn test_entry_points() {
        let code = r#"
fun onInternalMessage() {
    // Internal message handler
}

fun onExternalMessage() {
    // External message handler
}

fun regular_function() {
    // Just a regular function
}
"#;

        let tree = parse(code).unwrap();
        let root_node = tree.root_node();
        let abi = contract_abi(&root_node, code, "test.tolk");

        assert!(abi.entry_point.is_some());
        assert!(abi.external_entry_point.is_some());

        assert_eq!(
            abi.entry_point.as_ref().unwrap().pos.as_ref().unwrap().row,
            1
        );
        assert_eq!(
            abi.external_entry_point
                .as_ref()
                .unwrap()
                .pos
                .as_ref()
                .unwrap()
                .row,
            5
        );
    }

    #[test]
    fn test_method_id_formats() {
        let code = r#"
// Decimal method ID
@method_id(65537)
get fun decimal_id(): int {
    return 1;
}

// Hex method ID
@method_id(0x10001)
get fun hex_id(): int {
    return 2;
}

// Large method ID
@method_id(0xFFFFFFFF)
get fun large_id(): int {
    return 3;
}
"#;

        let tree = parse(code).unwrap();
        let root_node = tree.root_node();
        let abi = contract_abi(&root_node, code, "test.tolk");

        assert_eq!(abi.get_methods.len(), 3);

        let decimal_method = abi
            .get_methods
            .iter()
            .find(|m| m.name == "decimal_id")
            .unwrap();
        assert_eq!(decimal_method.id, 65537);

        let hex_method = abi.get_methods.iter().find(|m| m.name == "hex_id").unwrap();
        assert_eq!(hex_method.id, 0x10001);

        let large_method = abi
            .get_methods
            .iter()
            .find(|m| m.name == "large_id")
            .unwrap();
        assert_eq!(large_method.id, 0xFFFF_FFFF);
    }

    #[test]
    fn test_crc16_consistency() {
        let test_name = "get_balance";
        let crc_value = CRC16.checksum(test_name.as_bytes()) as u32;
        let method_id = (crc_value & 0xFFFF) | 0x10000;

        println!("CRC16 of '{}': 0x{:04x}", test_name, crc_value);
        println!("Method ID: 0x{:08x}", method_id);

        assert!(crc_value > 0);
        assert!(method_id >= 0x10000);
        assert_eq!(method_id & 0xFFFF, crc_value & 0xFFFF);
    }
}
