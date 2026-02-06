use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use serde::Serialize;
use std::io::Cursor;

use capitoltraders_lib::types::{IssuerDetail, PoliticianDetail, Trade};

/// Singularize common array field names for XML child elements.
fn singular(field: &str) -> &str {
    match field {
        "committees" => "committee",
        "labels" => "label",
        "eodPrices" => "priceSet",
        _ => field,
    }
}

/// Recursively write a serde_json::Value as XML elements.
fn write_value<W: std::io::Write>(
    writer: &mut Writer<W>,
    tag: &str,
    value: &serde_json::Value,
) -> Result<(), quick_xml::Error> {
    match value {
        serde_json::Value::Null => {
            // Omit null fields entirely
        }
        serde_json::Value::Bool(b) => {
            writer.write_event(Event::Start(BytesStart::new(tag)))?;
            writer.write_event(Event::Text(BytesText::new(if *b {
                "true"
            } else {
                "false"
            })))?;
            writer.write_event(Event::End(BytesEnd::new(tag)))?;
        }
        serde_json::Value::Number(n) => {
            writer.write_event(Event::Start(BytesStart::new(tag)))?;
            let s = n.to_string();
            writer.write_event(Event::Text(BytesText::new(&s)))?;
            writer.write_event(Event::End(BytesEnd::new(tag)))?;
        }
        serde_json::Value::String(s) => {
            writer.write_event(Event::Start(BytesStart::new(tag)))?;
            writer.write_event(Event::Text(BytesText::new(s)))?;
            writer.write_event(Event::End(BytesEnd::new(tag)))?;
        }
        serde_json::Value::Array(arr) => {
            writer.write_event(Event::Start(BytesStart::new(tag)))?;
            let child_tag = singular(tag);
            for item in arr {
                write_value(writer, child_tag, item)?;
            }
            writer.write_event(Event::End(BytesEnd::new(tag)))?;
        }
        serde_json::Value::Object(map) => {
            writer.write_event(Event::Start(BytesStart::new(tag)))?;
            for (key, val) in map {
                write_value(writer, key, val)?;
            }
            writer.write_event(Event::End(BytesEnd::new(tag)))?;
        }
    }
    Ok(())
}

/// Serialize a slice of Serialize items into an XML string.
fn items_to_xml<T: Serialize>(
    root_tag: &str,
    item_tag: &str,
    items: &[T],
) -> String {
    let mut writer = Writer::new_with_indent(Cursor::new(Vec::new()), b' ', 2);

    writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .expect("write xml decl");

    if items.is_empty() {
        writer
            .write_event(Event::Empty(BytesStart::new(root_tag)))
            .expect("write empty root");
    } else {
        writer
            .write_event(Event::Start(BytesStart::new(root_tag)))
            .expect("write root start");

        for item in items {
            let val = serde_json::to_value(item).expect("serialize to json value");
            write_value(&mut writer, item_tag, &val).expect("write xml value");
        }

        writer
            .write_event(Event::End(BytesEnd::new(root_tag)))
            .expect("write root end");
    }

    let buf = writer.into_inner().into_inner();
    String::from_utf8(buf).expect("valid utf8")
}

pub fn trades_to_xml(trades: &[Trade]) -> String {
    items_to_xml("trades", "trade", trades)
}

pub fn politicians_to_xml(politicians: &[PoliticianDetail]) -> String {
    items_to_xml("politicians", "politician", politicians)
}

pub fn issuers_to_xml(issuers: &[IssuerDetail]) -> String {
    items_to_xml("issuers", "issuer", issuers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_trades_fixture() -> Vec<Trade> {
        let json_str = include_str!("../../capitoltrades_api/tests/fixtures/trades.json");
        let resp: serde_json::Value = serde_json::from_str(json_str).unwrap();
        serde_json::from_value(resp["data"].clone()).unwrap()
    }

    fn load_politicians_fixture() -> Vec<PoliticianDetail> {
        let json_str = include_str!("../../capitoltrades_api/tests/fixtures/politicians.json");
        let resp: serde_json::Value = serde_json::from_str(json_str).unwrap();
        serde_json::from_value(resp["data"].clone()).unwrap()
    }

    fn load_issuers_fixture() -> Vec<IssuerDetail> {
        let json_str = include_str!("../../capitoltrades_api/tests/fixtures/issuers.json");
        let resp: serde_json::Value = serde_json::from_str(json_str).unwrap();
        serde_json::from_value(resp["data"].clone()).unwrap()
    }

    #[test]
    fn test_trade_xml_wellformed() {
        let trades = load_trades_fixture();
        let xml = trades_to_xml(&trades);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<trades>"));
        assert!(xml.contains("</trades>"));
        assert!(xml.contains("<trade>"));
        assert!(xml.contains("<_txId>12345</_txId>"));
        assert!(xml.contains("<txType>buy</txType>"));
        assert!(xml.contains("<issuerName>Apple Inc</issuerName>"));
    }

    #[test]
    fn test_politician_xml_output() {
        let politicians = load_politicians_fixture();
        let xml = politicians_to_xml(&politicians);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<politicians>"));
        assert!(xml.contains("<politician>"));
        assert!(xml.contains("<firstName>Nancy</firstName>"));
        assert!(xml.contains("<lastName>Pelosi</lastName>"));
        assert!(xml.contains("<countTrades>250</countTrades>"));
    }

    #[test]
    fn test_issuer_xml_output() {
        let issuers = load_issuers_fixture();
        let xml = issuers_to_xml(&issuers);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<issuers>"));
        assert!(xml.contains("<issuer>"));
        assert!(xml.contains("<issuerName>Apple Inc</issuerName>"));
        assert!(xml.contains("<mcap>2800000000000</mcap>"));
    }

    #[test]
    fn test_null_fields_omitted() {
        let trades = load_trades_fixture();
        let xml = trades_to_xml(&trades);
        // txTypeExtended is null in fixture, should not appear
        assert!(!xml.contains("<txTypeExtended>"));
        // comment is null in fixture, should not appear
        assert!(!xml.contains("<comment>"));
    }

    #[test]
    fn test_empty_array_produces_self_closing_root() {
        let xml = trades_to_xml(&[]);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<trades/>"));
        assert!(!xml.contains("</trades>"));
    }

    #[test]
    fn test_xml_special_chars_escaped() {
        // The quick-xml BytesText automatically escapes &, <, >
        // Verify by checking that the library handles it (we test via the write_value path)
        let mut writer = Writer::new(Cursor::new(Vec::new()));
        write_value(
            &mut writer,
            "test",
            &serde_json::Value::String("AT&T <Corp> \"quoted\"".to_string()),
        )
        .unwrap();
        let buf = writer.into_inner().into_inner();
        let xml = String::from_utf8(buf).unwrap();
        assert!(xml.contains("&amp;"));
        assert!(xml.contains("&lt;"));
        assert!(xml.contains("&gt;"));
        assert!(!xml.contains("AT&T <Corp>"));
    }

    // -- Round-trip parsing tests --

    fn assert_xml_parseable(xml: &str) {
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);
        loop {
            match reader.read_event() {
                Ok(Event::Eof) => break,
                Err(e) => panic!(
                    "XML parse error at position {}: {e}",
                    reader.error_position()
                ),
                _ => {}
            }
        }
    }

    #[test]
    fn test_trade_xml_parseable() {
        let trades = load_trades_fixture();
        let xml = trades_to_xml(&trades);
        assert_xml_parseable(&xml);
    }

    #[test]
    fn test_politician_xml_parseable() {
        let politicians = load_politicians_fixture();
        let xml = politicians_to_xml(&politicians);
        assert_xml_parseable(&xml);
    }

    #[test]
    fn test_issuer_xml_parseable() {
        let issuers = load_issuers_fixture();
        let xml = issuers_to_xml(&issuers);
        assert_xml_parseable(&xml);
    }

    // -- Structural verification tests --

    /// Collect element names at a given depth from an XML string.
    fn collect_element_names_at_depth(xml: &str, target_depth: usize) -> Vec<String> {
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);
        let mut depth: usize = 0;
        let mut names = Vec::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) => {
                    if depth == target_depth {
                        let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        names.push(name);
                    }
                    depth += 1;
                }
                Ok(Event::End(_)) => {
                    depth -= 1;
                }
                Ok(Event::Empty(e)) => {
                    if depth == target_depth {
                        let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        names.push(name);
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => panic!("XML parse error: {e}"),
                _ => {}
            }
        }
        names
    }

    /// Collect direct child element names under a given parent element.
    fn collect_children_of(xml: &str, parent: &str) -> Vec<String> {
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);
        let mut inside_parent = false;
        let mut parent_depth: usize = 0;
        let mut current_depth: usize = 0;
        let mut children = Vec::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    if inside_parent && current_depth == parent_depth + 1 {
                        children.push(name.clone());
                    }
                    if name == parent && !inside_parent {
                        inside_parent = true;
                        parent_depth = current_depth;
                    }
                    current_depth += 1;
                }
                Ok(Event::End(e)) => {
                    current_depth -= 1;
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    if inside_parent && name == parent && current_depth == parent_depth {
                        break;
                    }
                }
                Ok(Event::Empty(e)) => {
                    if inside_parent && current_depth == parent_depth + 1 {
                        let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        children.push(name);
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => panic!("XML parse error: {e}"),
                _ => {}
            }
        }
        children
    }

    #[test]
    fn test_trade_xml_field_presence() {
        let trades = load_trades_fixture();
        let xml = trades_to_xml(&trades);

        // Depth 2 = fields inside <trades><trade><FIELD>
        let fields = collect_element_names_at_depth(&xml, 2);

        let required = [
            "_txId",
            "_politicianId",
            "_assetId",
            "_issuerId",
            "pubDate",
            "filingDate",
            "txDate",
            "txType",
            "hasCapitalGains",
            "owner",
            "chamber",
            "value",
            "filingId",
            "filingURL",
            "reportingGap",
            "committees",
            "asset",
            "issuer",
            "politician",
            "labels",
        ];
        for name in required {
            assert!(
                fields.contains(&name.to_string()),
                "trade XML missing required field: {name}"
            );
        }
    }

    #[test]
    fn test_issuer_xml_nested_performance() {
        let issuers = load_issuers_fixture();
        let xml = issuers_to_xml(&issuers);

        let children = collect_children_of(&xml, "performance");
        assert!(
            children.contains(&"mcap".to_string()),
            "performance missing mcap"
        );
        assert!(
            children.contains(&"trailing1".to_string()),
            "performance missing trailing1"
        );
        assert!(
            children.contains(&"eodPrices".to_string()),
            "performance missing eodPrices"
        );
    }

    #[test]
    fn test_xml_array_child_singularization() {
        let trades = load_trades_fixture();
        let trade_xml = trades_to_xml(&trades);

        // <committees> should contain <committee> children
        let committee_children = collect_children_of(&trade_xml, "committees");
        assert!(
            committee_children.iter().all(|c| c == "committee"),
            "committees children should be <committee>, got: {committee_children:?}"
        );
        assert!(
            !committee_children.is_empty(),
            "committees should have children"
        );

        // <labels> should contain <label> children
        let label_children = collect_children_of(&trade_xml, "labels");
        assert!(
            label_children.iter().all(|c| c == "label"),
            "labels children should be <label>, got: {label_children:?}"
        );

        // <eodPrices> should contain <priceSet> children
        let issuers = load_issuers_fixture();
        let issuer_xml = issuers_to_xml(&issuers);
        let eod_children = collect_children_of(&issuer_xml, "eodPrices");
        assert!(
            eod_children.iter().all(|c| c == "priceSet"),
            "eodPrices children should be <priceSet>, got: {eod_children:?}"
        );
    }
}
