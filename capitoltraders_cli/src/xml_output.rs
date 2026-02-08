//! XML serialization via a JSON-to-XML bridge.
//!
//! Types are first serialized to `serde_json::Value`, then walked recursively
//! to emit XML via `quick_xml::Writer`. This avoids modifying the vendored crate
//! with serde XML derives.

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
fn items_to_xml<T: Serialize>(root_tag: &str, item_tag: &str, items: &[T]) -> String {
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

/// Serializes a slice of trades into an XML string with `<trades>` as the root element.
pub fn trades_to_xml(trades: &[Trade]) -> String {
    items_to_xml("trades", "trade", trades)
}

/// Serializes a slice of politicians into an XML string with `<politicians>` as the root element.
pub fn politicians_to_xml(politicians: &[PoliticianDetail]) -> String {
    items_to_xml("politicians", "politician", politicians)
}

/// Serializes a slice of issuers into an XML string with `<issuers>` as the root element.
pub fn issuers_to_xml(issuers: &[IssuerDetail]) -> String {
    items_to_xml("issuers", "issuer", issuers)
}

#[cfg(test)]
#[path = "xml_output_tests.rs"]
mod tests;
