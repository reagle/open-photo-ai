//! An XMP packet as a tree that keeps every byte the policy does not touch exactly as it was written.
//!
//! Text runs and attribute values are stored **escaped, as written**; they are decoded only when read
//! ([`decode`]) and new ones are escaped when added ([`encode`]). Comments, processing instructions (the
//! `<?xpacket?>` wrapper among them), the XML declaration and CDATA sections are kept as markup. So serialising a tree
//! that was not edited gives back the packet it was parsed from, apart from attribute quotes, which are re-derived.
//!
//! **What is refused**, so that reading a packet cannot be made to cost more than its size: a `DOCTYPE` (and with it any
//! entity definition), a reference to any entity but the five XML predefines and character references, nesting deeper
//! than [`MAX_DEPTH`], and anything `quick-xml` does not accept as well-formed. Nothing is ever expanded.

use std::borrow::Cow;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

/// How deep elements may nest. Real XMP nests a handful of levels (`x:xmpmeta`, `rdf:RDF`, `rdf:Description`, a
/// property, an array, an item, a structure field); anything far deeper is not a photograph's metadata.
pub(crate) const MAX_DEPTH: usize = 64;

/// One node of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    Element(Element),
    /// Character data, escaped as written (entity and character references included).
    Text(String),
    /// A comment, processing instruction, declaration or CDATA section, verbatim.
    Markup(String),
}

/// An element: its qualified name and attributes as written, and its children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Element {
    pub(crate) name: String,
    /// `(qualified name, value escaped as written)`, in order. Changed only through
    /// [`attributes_mut`](Self::attributes_mut), which is what keeps [`raw`](Self::raw) honest.
    attributes: Vec<(String, String)>,
    /// The attributes exactly as written — whitespace, line breaks and quotes — while they are unchanged.
    raw: Option<String>,
    pub(crate) children: Vec<Node>,
    /// Written as `<name/>`: kept that way while it has no children.
    pub(crate) empty: bool,
}

impl Element {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), attributes: Vec::new(), raw: None, children: Vec::new(), empty: false }
    }

    pub(crate) fn attributes(&self) -> &[(String, String)] {
        &self.attributes
    }

    /// The attributes, to change: from then on they are written from this list rather than as they were.
    pub(crate) fn attributes_mut(&mut self) -> &mut Vec<(String, String)> {
        self.raw = None;
        &mut self.attributes
    }

    /// The elements among the children.
    pub(crate) fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|node| match node {
            Node::Element(element) => Some(element),
            _ => None,
        })
    }
}

/// Why a packet was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum TreeError {
    #[error("the XMP packet is not UTF-8")]
    NotUtf8,
    #[error("the XMP packet is not well-formed: {0}")]
    Malformed(String),
    #[error("the XMP packet declares a DOCTYPE")]
    DocType,
    #[error("the XMP packet refers to an undefined entity &{0};")]
    Entity(String),
    #[error("the XMP packet nests deeper than {MAX_DEPTH} elements")]
    TooDeep,
}

/// Parses `packet` into its top-level nodes.
pub(crate) fn parse(packet: &[u8]) -> Result<Vec<Node>, TreeError> {
    let text = std::str::from_utf8(packet).map_err(|_| TreeError::NotUtf8)?;
    let mut reader = Reader::from_str(text);
    let config = reader.config_mut();
    config.trim_text(false);
    config.expand_empty_elements = false;
    config.check_end_names = true;

    let malformed = |error: quick_xml::Error| TreeError::Malformed(error.to_string());
    let mut roots: Vec<Node> = Vec::new();
    let mut open: Vec<Element> = Vec::new();
    let push = |open: &mut Vec<Element>, roots: &mut Vec<Node>, node: Node| match open.last_mut() {
        Some(parent) => append(&mut parent.children, node),
        None => append(roots, node),
    };

    loop {
        match reader.read_event().map_err(malformed)? {
            Event::Start(start) => {
                if open.len() >= MAX_DEPTH {
                    return Err(TreeError::TooDeep);
                }
                open.push(element(&start, false)?);
            }
            Event::Empty(start) => {
                if open.len() >= MAX_DEPTH {
                    return Err(TreeError::TooDeep);
                }
                let element = element(&start, true)?;
                push(&mut open, &mut roots, Node::Element(element));
            }
            Event::End(_) => {
                let closed = open.pop().ok_or_else(|| TreeError::Malformed("an end tag with no start".into()))?;
                push(&mut open, &mut roots, Node::Element(closed));
            }
            Event::Text(text) => push(&mut open, &mut roots, Node::Text(xml_chars(&text))),
            Event::GeneralRef(reference) => {
                let name = &*reference;
                check_reference(name)?;
                push(&mut open, &mut roots, Node::Text(format!("&{name};")));
            }
            Event::CData(data) => {
                push(&mut open, &mut roots, Node::Markup(format!("<![CDATA[{}]]>", xml_chars(&data))));
            }
            Event::Comment(comment) => {
                push(&mut open, &mut roots, Node::Markup(format!("<!--{}-->", xml_chars(&comment))));
            }
            Event::PI(instruction) => {
                push(&mut open, &mut roots, Node::Markup(format!("<?{}?>", xml_chars(&instruction))));
            }
            Event::Decl(declaration) => {
                push(&mut open, &mut roots, Node::Markup(format!("<?{}?>", xml_chars(&declaration))));
            }
            Event::DocType(_) => return Err(TreeError::DocType),
            Event::Eof => break,
        }
    }
    if !open.is_empty() {
        return Err(TreeError::Malformed("an element is not closed".into()));
    }
    Ok(roots)
}

/// Appends `node`, joining a text run onto the one before it, so an entity reference does not split a value in two.
fn append(nodes: &mut Vec<Node>, node: Node) {
    if let (Some(Node::Text(before)), Node::Text(text)) = (nodes.last_mut(), &node) {
        before.push_str(text);
        return;
    }
    nodes.push(node);
}

/// An element's name and attributes, every attribute value checked to decode.
fn element(start: &BytesStart<'_>, empty: bool) -> Result<Element, TreeError> {
    let name = start.name().0.to_owned();
    check_name(&name)?;
    let mut attributes = Vec::new();
    for attribute in start.attributes().with_checks(true) {
        let attribute = attribute.map_err(|error| TreeError::Malformed(error.to_string()))?;
        check_name(attribute.key.0)?;
        decode(&attribute.value)?;
        attributes.push((attribute.key.0.to_owned(), xml_chars(&attribute.value)));
    }
    let raw = Some(xml_chars(start.attributes_raw()));
    Ok(Element { name, attributes, raw, children: Vec::new(), empty })
}

/// Refuses a name that is not an XML qualified name: one name, or two joined by a single colon, each starting with a
/// letter or `_` and going on with letters, digits, `-`, `.` and `_` (any non-ASCII letter or digit counting too).
/// `quick-xml` is lenient here, and a prefix such as `/ap` that it lets through would be written back as markup.
fn check_name(name: &str) -> Result<(), TreeError> {
    let part = |part: &str| {
        let mut chars = part.chars();
        chars.next().is_some_and(|first| first.is_alphabetic() || first == '_')
            && chars.all(|next| next.is_alphanumeric() || matches!(next, '-' | '.' | '_'))
    };
    let valid = match name.split_once(':') {
        Some((prefix, local)) => part(prefix) && part(local),
        None => part(name),
    };
    if valid { Ok(()) } else { Err(TreeError::Malformed(format!("{name:?} is not an XML name"))) }
}

/// A reference XML defines without a DTD: the five predefined entities, or a character reference.
fn check_reference(name: &str) -> Result<(), TreeError> {
    if matches!(name, "amp" | "lt" | "gt" | "apos" | "quot") {
        return Ok(());
    }
    let code = if let Some(hex) = name.strip_prefix("#x") {
        u32::from_str_radix(hex, 16).ok()
    } else {
        name.strip_prefix('#').and_then(|decimal| decimal.parse().ok())
    };
    code.and_then(char::from_u32)
        .filter(|&c| allowed(c))
        .map(|_| ())
        .ok_or_else(|| TreeError::Entity(name.to_owned()))
}

/// Whether XML 1.0 allows `c` in a document: tab, line feed, carriage return, and everything from space up except
/// U+FFFE and U+FFFF (Rust's `char` already excludes the surrogates).
pub(crate) fn allowed(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r') || (c >= ' ' && !matches!(c, '\u{FFFE}' | '\u{FFFF}'))
}

/// `text` without the characters XML 1.0 does not allow, which a lenient writer may have left in a packet.
fn xml_chars(text: &str) -> String {
    text.chars().filter(|&c| allowed(c)).collect()
}

/// Text or an attribute value as written, decoded: predefined entities and character references only.
pub(crate) fn decode(escaped: &str) -> Result<Cow<'_, str>, TreeError> {
    quick_xml::escape::unescape(escaped).map_err(|error| TreeError::Entity(error.to_string()))
}

/// `value` escaped for text or an attribute, without the characters XML 1.0 does not allow — the NUL padding and stray
/// control characters a Latin-1 IPTC record or an EXIF string may hold — which no escaping can make legal.
pub(crate) fn encode(value: &str) -> String {
    quick_xml::escape::escape(xml_chars(value)).into_owned()
}

/// Serialises `nodes`.
pub(crate) fn serialise(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        write_node(&mut out, node);
    }
    out
}

fn write_node(out: &mut String, node: &Node) {
    match node {
        Node::Text(text) | Node::Markup(text) => out.push_str(text),
        Node::Element(element) => {
            out.push('<');
            out.push_str(&element.name);
            if let Some(raw) = &element.raw {
                out.push_str(raw);
            } else {
                write_attributes(out, &element.attributes);
            }
            if element.empty && element.children.is_empty() {
                out.push_str("/>");
                return;
            }
            out.push('>');
            for child in &element.children {
                write_node(out, child);
            }
            out.push_str("</");
            out.push_str(&element.name);
            out.push('>');
        }
    }
}

fn write_attributes(out: &mut String, attributes: &[(String, String)]) {
    for (name, value) in attributes {
        // A value written between single quotes may hold a double one, and the other way round.
        let quote = if value.contains('"') { '\'' } else { '"' };
        out.push(' ');
        out.push_str(name);
        out.push('=');
        out.push(quote);
        out.push_str(value);
        out.push(quote);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A packet as ExifTool writes it: wrapper, comment, attribute and element forms, an `rdf:Bag`, an `rdf:Alt`,
    /// references, and padding.
    pub(crate) const EXIFTOOL: &str = "<?xpacket begin='\u{feff}' id='W5M0MpCehiHzreSzNTczkc9d'?>\n\
<x:xmpmeta xmlns:x='adobe:ns:meta/' x:xmptk='Image::ExifTool 13.55'>\n\
<!-- a comment -->\n\
<rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>\n\
 <rdf:Description rdf:about='' xmlns:xmp='http://ns.adobe.com/xap/1.0/' xmp:Rating='3' xmp:Label=\"say &quot;hi&quot;\"/>\n\
 <rdf:Description rdf:about=''\n  xmlns:dc='http://purl.org/dc/elements/1.1/'>\n\
  <dc:subject>\n   <rdf:Bag>\n    <rdf:li>hawk &amp; owl</rdf:li>\n    <rdf:li>caf&#233;</rdf:li>\n   </rdf:Bag>\n  </dc:subject>\n\
  <dc:title>\n   <rdf:Alt>\n    <rdf:li xml:lang='x-default'>Title &lt;1&gt;</rdf:li>\n   </rdf:Alt>\n  </dc:title>\n\
 </rdf:Description>\n\
</rdf:RDF>\n\
</x:xmpmeta>\n\
                                                                                \n\
<?xpacket end='w'?>";

    #[test]
    fn a_packet_that_is_not_edited_comes_back_byte_for_byte() {
        let nodes = parse(EXIFTOOL.as_bytes()).unwrap();
        assert_eq!(serialise(&nodes), EXIFTOOL);
    }

    #[test]
    fn edited_attributes_are_written_from_the_list_with_fitting_quotes() {
        let mut nodes = parse(b"<a  x='1'\n  y='say \"hi\"'/>").unwrap();
        let Node::Element(element) = &mut nodes[0] else { panic!("an element") };
        element.attributes_mut().push(("z".into(), "it&apos;s".into()));
        assert_eq!(serialise(&nodes), "<a x=\"1\" y='say \"hi\"' z=\"it&apos;s\"/>");
    }

    #[test]
    fn text_and_values_decode_on_demand() {
        assert_eq!(decode("hawk &amp; owl caf&#233; &#x263A;").unwrap(), "hawk & owl café ☺");
        assert_eq!(encode("a<b & \"c\""), "a&lt;b &amp; &quot;c&quot;");
        // Characters XML 1.0 forbids are dropped rather than written.
        assert_eq!(encode("bell\u{7} and nul\0\0"), "bell and nul");
        assert_eq!(encode("tab\tline\n\u{FFFE}"), "tab\tline\n");
        assert!(matches!(parse(b"<a>&#1;</a>"), Err(TreeError::Entity(_))));
        assert!(matches!(parse(b"<a>&#xFFFE;</a>"), Err(TreeError::Entity(_))));
        // Raw ones in a packet are dropped on the way in.
        assert_eq!(serialise(&parse(b"<a b='x\x01y'>p\x07q</a>").unwrap()), "<a b='xy'>pq</a>");
        assert!(decode("&custom;").is_err());
    }

    #[test]
    fn a_doctype_an_undefined_entity_and_deep_nesting_are_refused() {
        let laughs = "<?xml version='1.0'?><!DOCTYPE lolz [<!ENTITY lol 'lol'><!ENTITY lol2 '&lol;&lol;&lol;'>]>\
                      <x:xmpmeta xmlns:x='adobe:ns:meta/'>&lol2;</x:xmpmeta>";
        assert_eq!(parse(laughs.as_bytes()), Err(TreeError::DocType));
        assert!(matches!(parse(b"<a>&lol;</a>"), Err(TreeError::Entity(_))));
        assert!(matches!(parse(b"<a b='&lol;'/>"), Err(TreeError::Entity(_))));
        let deep = format!("{}{}", "<a>".repeat(MAX_DEPTH + 1), "</a>".repeat(MAX_DEPTH + 1));
        assert_eq!(parse(deep.as_bytes()), Err(TreeError::TooDeep));
        let fine = format!("{}{}", "<a>".repeat(MAX_DEPTH), "</a>".repeat(MAX_DEPTH));
        assert!(parse(fine.as_bytes()).is_ok());
        assert_eq!(parse(b"\xff\xfe"), Err(TreeError::NotUtf8));
        assert!(matches!(parse(b"<a><b></a>"), Err(TreeError::Malformed(_))));
        assert!(matches!(parse(b"<a>"), Err(TreeError::Malformed(_))));
        assert!(matches!(parse(b"<a x='1' x='2'/>"), Err(TreeError::Malformed(_))));
        assert!(matches!(parse(b"<a xmlns:/ap='u'/>"), Err(TreeError::Malformed(_))));
        assert!(matches!(parse(b"<a:b:c/>"), Err(TreeError::Malformed(_))));
        assert!(matches!(parse(b"<a 1x='1'/>"), Err(TreeError::Malformed(_))));
        assert!(parse("<café:x xml:lang='en' _a.b-c='1'/>".as_bytes()).is_ok());
    }

    #[test]
    fn truncations_and_byte_flips_never_panic() {
        let packet = EXIFTOOL.as_bytes();
        for len in 0..packet.len() {
            if let Ok(nodes) = parse(&packet[..len]) {
                let _ = serialise(&nodes);
            }
        }
        let mut flipped = packet.to_vec();
        for at in 0..flipped.len() {
            for value in [b'<', b'>', b'&', b'"', b'\'', b'/', 0, 0xFF] {
                let original = std::mem::replace(&mut flipped[at], value);
                if let Ok(nodes) = parse(&flipped) {
                    // What parses serialises to something that parses again.
                    assert!(parse(serialise(&nodes).as_bytes()).is_ok());
                }
                flipped[at] = original;
            }
        }
    }
}
