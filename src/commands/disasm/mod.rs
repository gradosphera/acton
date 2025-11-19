use std::fs;
use tasm::decompile::Disassembler;
use tasm::printer::FormatOptions;
use tycho_types::boc::Boc;

mod toncenter;

pub fn disasm_cmd(
    boc_file: Option<String>,
    boc_string: Option<String>,
    output_file: Option<String>,
    opts: FormatOptions,
    address: Option<String>,
    api_key: Option<String>,
) -> anyhow::Result<()> {
    let boc_data = if let Some(string) = boc_string {
        string
    } else if let Some(file_path) = boc_file {
        let binary_data = fs::read(&file_path)?;
        hex::encode(binary_data)
    } else if let Some(addr) = address {
        toncenter::fetch_contract_boc(&addr, api_key.as_deref())?
    } else {
        return Err(anyhow::anyhow!(
            "Either --string/-s, --address or boc_file must be provided"
        ));
    };

    let cell = if let Ok(cell) = Boc::decode_hex(&boc_data) {
        cell
    } else if let Ok(cell) = Boc::decode_base64(&boc_data) {
        cell
    } else {
        return Err(anyhow::anyhow!(
            "Failed to decode BOC data as hex or base64"
        ));
    };

    let disassembler = Disassembler::new();
    let code = disassembler.decompile_cell(&cell)?;

    let output = code.print(&opts);

    if let Some(output_path) = output_file {
        fs::write(&output_path, &output)?;
        println!("Disassembled code written to {output_path}");
    } else {
        println!("{output}");
    }

    Ok(())
}
