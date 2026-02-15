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
use capitoltraders_lib::{
    ContributorAggRow, DbIssuerRow, DbPoliticianRow, DbTradeRow, DonationRow, EmployerAggRow,
    PortfolioPosition, StateAggRow,
};

use crate::commands::analytics::LeaderboardRow;
use crate::commands::conflicts::{ConflictRow, DonationCorrelationRow};

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

/// Serializes a slice of DB trade rows into an XML string with `<trades>` as the root element.
#[allow(dead_code)]
pub fn db_trades_to_xml(trades: &[DbTradeRow]) -> String {
    items_to_xml("trades", "trade", trades)
}

/// Serializes enriched DB trade rows (with analytics) into an XML string with `<trades>` as the root element.
pub fn enriched_trades_to_xml(trades: &[crate::commands::trades::EnrichedDbTradeRow]) -> String {
    items_to_xml("trades", "trade", trades)
}

/// Serializes a slice of DB politician rows into an XML string with `<politicians>` as the root element.
#[allow(dead_code)]
pub fn db_politicians_to_xml(politicians: &[DbPoliticianRow]) -> String {
    items_to_xml("politicians", "politician", politicians)
}

/// Serializes enriched DB politician rows (with analytics) into an XML string with `<politicians>` as the root element.
pub fn enriched_politicians_to_xml(
    politicians: &[crate::commands::politicians::EnrichedDbPoliticianRow],
) -> String {
    items_to_xml("politicians", "politician", politicians)
}

/// Serializes a slice of DB issuer rows into an XML string with `<issuers>` as the root element.
pub fn db_issuers_to_xml(issuers: &[DbIssuerRow]) -> String {
    items_to_xml("issuers", "issuer", issuers)
}

/// Serializes a slice of portfolio positions into an XML string with `<portfolio>` as the root element.
#[allow(dead_code)]
pub fn portfolio_to_xml(positions: &[PortfolioPosition]) -> String {
    items_to_xml("portfolio", "position", positions)
}

/// Serializes enriched portfolio positions (with conflict detection) into an XML string with `<portfolio>` as the root element.
pub fn enriched_portfolio_to_xml(
    positions: &[crate::commands::portfolio::EnrichedPortfolioPosition],
) -> String {
    items_to_xml("portfolio", "position", positions)
}

/// Serializes donations into XML with `<donations>` root element.
pub fn donations_to_xml(donations: &[DonationRow]) -> String {
    items_to_xml("donations", "donation", donations)
}

/// Serializes contributor aggregations into XML.
pub fn contributor_agg_to_xml(rows: &[ContributorAggRow]) -> String {
    items_to_xml("contributors", "contributor", rows)
}

/// Serializes employer aggregations into XML.
pub fn employer_agg_to_xml(rows: &[EmployerAggRow]) -> String {
    items_to_xml("employers", "employer", rows)
}

/// Serializes state aggregations into XML.
pub fn state_agg_to_xml(rows: &[StateAggRow]) -> String {
    items_to_xml("states", "state", rows)
}

/// Serializes leaderboard into XML with `<leaderboard>` root element.
pub fn leaderboard_to_xml(rows: &[LeaderboardRow]) -> String {
    items_to_xml("leaderboard", "politician", rows)
}

/// Serializes conflict rows into XML with `<conflicts>` root element.
pub fn conflicts_to_xml(rows: &[ConflictRow]) -> String {
    items_to_xml("conflicts", "conflict", rows)
}

/// Serializes donation correlations into XML with `<donation_correlations>` root element.
pub fn donation_correlations_to_xml(rows: &[DonationCorrelationRow]) -> String {
    items_to_xml("donation_correlations", "correlation", rows)
}

#[cfg(test)]
#[path = "xml_output_tests.rs"]
mod tests;
