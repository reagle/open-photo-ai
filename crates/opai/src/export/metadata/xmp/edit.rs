//! Reading and changing the properties of an XMP packet's top-level `rdf:Description`s, by namespace URI.
//!
//! A property may be written as an attribute of an `rdf:Description` (`xmp:Rating="3"`) or as a child element
//! (`<xmp:Rating>3</xmp:Rating>`), in any of several descriptions, under any prefix the packet binds to its namespace.
//! Every operation here looks in all of them. A property that is set is first removed wherever it is, then written as
//! an element of the first description, with its namespace declared there if the packet has not bound it in scope.
//!
//! Only the top level is edited. A property's own content — an array, a structure, qualifiers — is carried as written
//! unless the property as a whole is replaced or appended to.

use super::tree::{self, Element, Node, TreeError};

/// The namespaces the policy names.
pub(crate) mod ns {
    pub(crate) const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    pub(crate) const XMP: &str = "http://ns.adobe.com/xap/1.0/";
    pub(crate) const XMP_MM: &str = "http://ns.adobe.com/xap/1.0/mm/";
    pub(crate) const ST_REF: &str = "http://ns.adobe.com/xap/1.0/sType/ResourceRef#";
    pub(crate) const ST_EVT: &str = "http://ns.adobe.com/xap/1.0/sType/ResourceEvent#";
    pub(crate) const XMP_NOTE: &str = "http://ns.adobe.com/xmp/note/";
    pub(crate) const TIFF: &str = "http://ns.adobe.com/tiff/1.0/";
    pub(crate) const EXIF: &str = "http://ns.adobe.com/exif/1.0/";
    pub(crate) const CRS: &str = "http://ns.adobe.com/camera-raw-settings/1.0/";
    pub(crate) const HDRGM: &str = "http://ns.adobe.com/hdr-gain-map/1.0/";
    pub(crate) const CONTAINER: &str = "http://ns.google.com/photos/1.0/container/";
    pub(crate) const ITEM: &str = "http://ns.google.com/photos/1.0/container/item/";
    pub(crate) const GCAMERA: &str = "http://ns.google.com/photos/1.0/camera/";
    pub(crate) const GIMAGE: &str = "http://ns.google.com/photos/1.0/image/";
    pub(crate) const GFOCUS: &str = "http://ns.google.com/photos/1.0/focus/";
    pub(crate) const GDEPTH: &str = "http://ns.google.com/photos/1.0/depthmap/";
    pub(crate) const DC: &str = "http://purl.org/dc/elements/1.1/";
    pub(crate) const PHOTOSHOP: &str = "http://ns.adobe.com/photoshop/1.0/";
    pub(crate) const IPTC_CORE: &str = "http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/";
}

/// The prefix a namespace is declared under when the packet has none for it.
fn preferred_prefix(uri: &str) -> &'static str {
    match uri {
        ns::RDF => "rdf",
        ns::XMP => "xmp",
        ns::XMP_MM => "xmpMM",
        ns::ST_REF => "stRef",
        ns::ST_EVT => "stEvt",
        ns::TIFF => "tiff",
        ns::EXIF => "exif",
        ns::DC => "dc",
        ns::PHOTOSHOP => "photoshop",
        ns::IPTC_CORE => "Iptc4xmpCore",
        _ => "ns",
    }
}

/// The namespace declarations in scope, innermost last: `(prefix, URI)`, `""` for the default namespace.
#[derive(Debug, Clone, Default)]
struct Scope(Vec<(String, String)>);

impl Scope {
    /// This scope with `attributes`' declarations added.
    fn with(&self, attributes: &[(String, String)]) -> Scope {
        let mut scope = self.clone();
        for (name, value) in attributes {
            let prefix = if name == "xmlns" { Some("") } else { name.strip_prefix("xmlns:") };
            if let Some(prefix) = prefix {
                let uri = tree::decode(value).map(|uri| uri.into_owned()).unwrap_or_default();
                scope.0.push((prefix.to_owned(), uri));
            }
        }
        scope
    }

    fn uri(&self, prefix: &str) -> Option<&str> {
        self.0.iter().rev().find(|(bound, _)| bound == prefix).map(|(_, uri)| uri.as_str())
    }

    /// A prefix that resolves to `uri` here, if any does.
    fn prefix_for(&self, uri: &str) -> Option<&str> {
        self.0
            .iter()
            .rev()
            .map(|(prefix, _)| prefix.as_str())
            .find(|prefix| !prefix.is_empty() && self.uri(prefix) == Some(uri))
    }

    /// An element's `(namespace URI, local name)`; an unprefixed one is in the default namespace.
    fn element<'a>(&'a self, name: &'a str) -> Option<(&'a str, &'a str)> {
        match name.split_once(':') {
            Some((prefix, local)) => Some((self.uri(prefix)?, local)),
            None => Some((self.uri("")?, name)),
        }
    }

    /// An attribute's `(namespace URI, local name)`; an unprefixed one is in no namespace, and declarations are not
    /// attributes of anything.
    fn attribute<'a>(&'a self, name: &'a str) -> Option<(&'a str, &'a str)> {
        if name == "xmlns" || name.starts_with("xmlns:") {
            return None;
        }
        let (prefix, local) = name.split_once(':')?;
        Some((self.uri(prefix)?, local))
    }
}

/// An array's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Array {
    /// Unordered: keywords.
    Bag,
    /// Ordered: creators, history.
    Seq,
    /// Alternatives by language: titles, descriptions, rights; written with the one `x-default` item.
    Alt,
}

/// One field of a structure: `(namespace, local name, value)`.
pub(crate) type Field<'a> = (&'a str, &'a str, String);

/// A parsed packet with its `rdf:RDF` found.
#[derive(Debug, Clone)]
pub(crate) struct Xmp {
    nodes: Vec<Node>,
    /// The child indices from the top level down to `rdf:RDF`.
    rdf: Vec<usize>,
    /// The namespaces in scope at `rdf:RDF`, its own declarations included.
    scope: Scope,
}

impl Xmp {
    /// Parses `packet`. Refused as [`TreeError::Malformed`] if it has no `rdf:RDF`.
    pub(crate) fn parse(packet: &[u8]) -> Result<Self, TreeError> {
        let nodes = tree::parse(packet)?;
        let (rdf, scope) = find_rdf(&nodes, &Scope::default(), &mut Vec::new())
            .ok_or_else(|| TreeError::Malformed("no rdf:RDF".into()))?;
        let mut xmp = Self { nodes, rdf, scope };
        if xmp.descriptions().next().is_none() {
            // Under a prefix bound to the RDF namespace — declared on the description itself where the packet binds
            // none, as when `rdf:RDF` is written with RDF as its default namespace.
            let mut description = Element::new(String::new());
            let prefixes = declare_all(&mut description, &xmp.scope, &[ns::RDF]);
            let rdf = prefix_of(&prefixes, ns::RDF).to_owned();
            description.name = format!("{rdf}:Description");
            description.attributes_mut().push((format!("{rdf}:about"), String::new()));
            insert(&mut xmp.rdf_mut().children, Node::Element(description));
        }
        if xmp.descriptions().next().is_none() {
            return Err(TreeError::Malformed("no rdf:Description could be added".into()));
        }
        Ok(xmp)
    }

    /// An empty packet: the wrapper, `x:xmpmeta`, `rdf:RDF` and one empty `rdf:Description`.
    pub(crate) fn empty() -> Self {
        let packet = "<?xpacket begin='\u{feff}' id='W5M0MpCehiHzreSzNTczkc9d'?>\n\
                      <x:xmpmeta xmlns:x='adobe:ns:meta/'>\n\
                      <rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>\n \
                      <rdf:Description rdf:about=''>\n \
                      </rdf:Description>\n\
                      </rdf:RDF>\n\
                      </x:xmpmeta>\n\
                      <?xpacket end='w'?>";
        Self::parse(packet.as_bytes()).expect("the template parses")
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        tree::serialise(&self.nodes).into_bytes()
    }

    fn rdf(&self) -> &Element {
        let mut nodes = &self.nodes;
        let mut found = None;
        for &index in &self.rdf {
            let Node::Element(element) = &nodes[index] else {
                unreachable!("the path leads through elements")
            };
            nodes = &element.children;
            found = Some(element);
        }
        found.expect("the path is not empty")
    }

    fn rdf_mut(&mut self) -> &mut Element {
        let mut nodes = &mut self.nodes;
        let (last, path) = self.rdf.split_last().expect("the path is not empty");
        for &index in path {
            let Node::Element(element) = &mut nodes[index] else {
                unreachable!("the path leads through elements")
            };
            nodes = &mut element.children;
        }
        let Node::Element(element) = &mut nodes[*last] else {
            unreachable!("the path leads to an element")
        };
        element
    }

    /// The top-level descriptions, each with its scope.
    fn descriptions(&self) -> impl Iterator<Item = (&Element, Scope)> {
        let scope = &self.scope;
        self.rdf().elements().filter_map(move |element| {
            let inner = scope.with(element.attributes());
            (inner.element(&element.name) == Some((ns::RDF, "Description"))).then_some((element, inner))
        })
    }

    /// The child indices of the top-level descriptions under `rdf:RDF`.
    fn description_indices(&self) -> Vec<usize> {
        let scope = &self.scope;
        self.rdf()
            .children
            .iter()
            .enumerate()
            .filter_map(|(index, node)| match node {
                Node::Element(element)
                    if scope.with(element.attributes()).element(&element.name) == Some((ns::RDF, "Description")) =>
                {
                    Some(index)
                }
                _ => None,
            })
            .collect()
    }

    fn description_mut(&mut self, index: usize) -> (&mut Element, Scope) {
        let scope = self.scope.clone();
        let Node::Element(element) = &mut self.rdf_mut().children[index] else {
            unreachable!("a description")
        };
        let inner = scope.with(element.attributes());
        (element, inner)
    }

    /// Whether `ns:local` is present, in either form.
    pub(crate) fn has(&self, ns: &str, local: &str) -> bool {
        self.descriptions().any(|(description, scope)| {
            description.attributes().iter().any(|(name, _)| scope.attribute(name) == Some((ns, local)))
                || description
                    .elements()
                    .any(|child| scope.with(child.attributes()).element(&child.name) == Some((ns, local)))
        })
    }

    /// The decoded value of the simple property `ns:local`: an attribute, or an element holding only text.
    pub(crate) fn get(&self, ns: &str, local: &str) -> Option<String> {
        for (description, scope) in self.descriptions() {
            for (name, value) in description.attributes() {
                if scope.attribute(name) == Some((ns, local)) {
                    return tree::decode(value).ok().map(|value| value.into_owned());
                }
            }
            for child in description.elements() {
                if scope.with(child.attributes()).element(&child.name) == Some((ns, local)) {
                    return simple_text(child);
                }
            }
        }
        None
    }

    /// Removes `ns:local` wherever it is.
    pub(crate) fn remove(&mut self, ns: &str, local: &str) {
        self.remove_where(|uri, name| uri == ns && name == local);
    }

    /// Removes every property in `ns` whose local name starts with `start` (`GCamera:MotionPhoto*`).
    pub(crate) fn remove_starting(&mut self, ns: &str, start: &str) {
        self.remove_where(|uri, name| uri == ns && name.starts_with(start));
    }

    /// Edits the items of the ordered array `ns:local` — each `rdf:li` of the `rdf:Seq` a description's element of that
    /// name holds — one at a time: `edit` may read and change the fields of each through [`SeqItem`], and answers
    /// whether to keep it. A removed item takes its indentation with it. Answers whether the array was found.
    pub(crate) fn edit_seq(&mut self, ns: &str, local: &str, mut edit: impl FnMut(&mut SeqItem<'_>) -> bool) -> bool {
        let mut found = false;
        for index in self.description_indices() {
            let (description, scope) = self.description_mut(index);
            for node in &mut description.children {
                let Node::Element(property) = node else { continue };
                let inner = scope.with(property.attributes());
                if inner.element(&property.name) != Some((ns, local)) {
                    continue;
                }
                for node in &mut property.children {
                    let Node::Element(seq) = node else { continue };
                    let seq_scope = inner.with(seq.attributes());
                    if seq_scope.element(&seq.name) != Some((ns::RDF, "Seq")) {
                        continue;
                    }
                    found = true;
                    let mut kept: Vec<Node> = Vec::with_capacity(seq.children.len());
                    for node in std::mem::take(&mut seq.children) {
                        let keep = match node {
                            Node::Element(mut li)
                                if seq_scope.with(li.attributes()).element(&li.name) == Some((ns::RDF, "li")) =>
                            {
                                let item_scope = seq_scope.with(li.attributes());
                                let keep = edit(&mut SeqItem { element: &mut li, scope: item_scope });
                                keep.then_some(Node::Element(li))
                            }
                            other => Some(other),
                        };
                        match keep {
                            Some(node) => kept.push(node),
                            None => {
                                if matches!(kept.last(), Some(Node::Text(text)) if text.trim().is_empty()) {
                                    kept.pop();
                                }
                            }
                        }
                    }
                    seq.children = kept;
                }
            }
        }
        found
    }

    /// Removes every property in namespace `ns`, and then each declaration of it that nothing in its scope uses any
    /// more — so a packet that described a gain map no longer declares the gain-map namespace, which a reader may take
    /// as the claim itself.
    pub(crate) fn remove_namespace(&mut self, ns: &str) {
        self.remove_where(|uri, _| uri == ns);
        self.remove_namespace_if_unused(ns);
    }

    /// Removes each declaration of `ns` that nothing in its scope uses: on a description, on `rdf:RDF`, on
    /// `x:xmpmeta`, or anywhere else in the packet.
    pub(crate) fn remove_namespace_if_unused(&mut self, ns: &str) {
        remove_unused_declarations(&mut self.nodes, ns);
        // The declarations in scope at `rdf:RDF` may be among those removed; its path is not changed.
        if let Some((_, scope)) = find_rdf(&self.nodes, &Scope::default(), &mut Vec::new()) {
            self.scope = scope;
        }
    }

    fn remove_where(&mut self, matches: impl Fn(&str, &str) -> bool) {
        for index in self.description_indices() {
            let (description, scope) = self.description_mut(index);
            let attribute_matches = description
                .attributes()
                .iter()
                .any(|(name, _)| scope.attribute(name).is_some_and(|(uri, local)| matches(uri, local)));
            if attribute_matches {
                description
                    .attributes_mut()
                    .retain(|(name, _)| !scope.attribute(name).is_some_and(|(uri, local)| matches(uri, local)));
            }
            let mut kept: Vec<Node> = Vec::with_capacity(description.children.len());
            for node in std::mem::take(&mut description.children) {
                let removed = matches!(&node, Node::Element(child)
                    if scope.with(child.attributes()).element(&child.name).is_some_and(|(uri, local)| matches(uri, local)));
                if removed {
                    // The indentation in front of it goes too, so removals leave no blank lines.
                    if matches!(kept.last(), Some(Node::Text(text)) if text.trim().is_empty()) {
                        kept.pop();
                    }
                } else {
                    kept.push(node);
                }
            }
            description.children = kept;
        }
    }

    /// Sets the simple property `ns:local` to `value`.
    pub(crate) fn set(&mut self, ns: &str, local: &str, value: &str) {
        self.remove(ns, local);
        self.add_to_first(ns, local, |_, _| vec![Node::Text(tree::encode(value))], &[], false);
    }

    /// Sets the structure `ns:local` to `fields`.
    pub(crate) fn set_struct(&mut self, ns: &str, local: &str, fields: &[Field<'_>]) {
        self.remove(ns, local);
        let namespaces: Vec<&str> = fields.iter().map(|(uri, ..)| *uri).collect();
        self.add_to_first(ns, local, |prefixes, _| structure(fields, prefixes), &namespaces, true);
    }

    /// Sets the array `ns:local` to `items`, of `kind`.
    pub(crate) fn set_array(&mut self, ns: &str, local: &str, kind: Array, items: &[String]) {
        self.remove(ns, local);
        self.add_to_first(ns, local, |_, rdf| array(kind, items, rdf), &[], false);
    }

    /// The decoded text of each item of the ordered array `ns:local`, in order: `None` if it is absent, is not an
    /// `rdf:Seq`, or holds an item that is not plain text.
    pub(crate) fn seq_values(&self, ns: &str, local: &str) -> Option<Vec<String>> {
        for (description, scope) in self.descriptions() {
            for child in description.elements() {
                let inner = scope.with(child.attributes());
                if inner.element(&child.name) != Some((ns, local)) {
                    continue;
                }
                let seq = child.elements().find(|element| {
                    inner.with(element.attributes()).element(&element.name) == Some((ns::RDF, "Seq"))
                })?;
                let seq_scope = inner.with(seq.attributes());
                return seq
                    .elements()
                    .filter(|li| seq_scope.with(li.attributes()).element(&li.name) == Some((ns::RDF, "li")))
                    .map(simple_text)
                    .collect();
            }
        }
        None
    }

    /// How many items the array `ns:local` holds as an element of a description: `None` if it is absent or is not an
    /// `rdf:Seq`, `rdf:Bag` or `rdf:Alt`.
    pub(crate) fn array_len(&self, ns: &str, local: &str) -> Option<usize> {
        for (description, scope) in self.descriptions() {
            for child in description.elements() {
                let inner = scope.with(child.attributes());
                if inner.element(&child.name) == Some((ns, local)) {
                    let container = child.elements().find(|element| {
                        matches!(
                            inner.with(element.attributes()).element(&element.name),
                            Some((ns::RDF, "Seq" | "Bag" | "Alt"))
                        )
                    })?;
                    return Some(container.elements().count());
                }
            }
        }
        None
    }

    /// Appends a structure of `fields` as a new item of the ordered array `ns:local`, creating the array if it is
    /// absent. An existing property that is not an `rdf:Seq` is left as it is and nothing is appended: there is no
    /// order to append to. Answers whether the item was appended.
    pub(crate) fn append_struct(&mut self, ns: &str, local: &str, fields: &[Field<'_>]) -> bool {
        let namespaces: Vec<&str> = fields.iter().map(|(uri, ..)| *uri).collect();
        for index in self.description_indices() {
            let (description, scope) = self.description_mut(index);
            let Some(position) = description.children.iter().position(|node| {
                matches!(node, Node::Element(child) if scope.with(child.attributes()).element(&child.name) == Some((ns, local)))
            }) else {
                continue;
            };
            let mut wanted = vec![ns::RDF];
            wanted.extend_from_slice(&namespaces);
            let prefixes = declare_all(description, &scope, &wanted);
            let scope = scope.with(description.attributes());
            let rdf = prefix_of(&prefixes, ns::RDF).to_owned();
            let Node::Element(property) = &mut description.children[position] else {
                unreachable!("found above")
            };
            let inner = scope.with(property.attributes());
            let Some(Node::Element(seq)) = property.children.iter_mut().find(|node| {
                matches!(node, Node::Element(element) if inner.with(element.attributes()).element(&element.name) == Some((ns::RDF, "Seq")))
            }) else {
                return false;
            };
            let mut item = Element::new(format!("{rdf}:li"));
            item.attributes_mut().push((format!("{rdf}:parseType"), "Resource".into()));
            item.children = structure(fields, &prefixes);
            insert(&mut seq.children, Node::Element(item));
            return true;
        }

        // Absent: a new `rdf:Seq` holding the one item.
        self.add_to_first(
            ns,
            local,
            |prefixes, rdf| {
                let mut item = Element::new(format!("{rdf}:li"));
                item.attributes_mut().push((format!("{rdf}:parseType"), "Resource".into()));
                item.children = structure(fields, prefixes);
                let mut seq = Element::new(format!("{rdf}:Seq"));
                insert(&mut seq.children, Node::Element(item));
                vec![Node::Element(seq)]
            },
            &namespaces,
            false,
        );
        true
    }

    /// Adds `ns:local` as an element of the first description, its content built by `content` from the prefixes of
    /// `namespaces` (declared as needed) and the `rdf` prefix; a structure (`resource`) is marked
    /// `rdf:parseType="Resource"`, which is what lets its fields be its children.
    fn add_to_first(
        &mut self,
        ns: &str,
        local: &str,
        content: impl FnOnce(&[(String, String)], &str) -> Vec<Node>,
        namespaces: &[&str],
        resource: bool,
    ) {
        // `parse` guarantees a description; should there be none, nothing is added rather than anything panicking.
        let Some(&index) = self.description_indices().first() else { return };
        let (description, scope) = self.description_mut(index);
        let mut all = vec![ns, ns::RDF];
        all.extend_from_slice(namespaces);
        let prefixes = declare_all(description, &scope, &all);
        let rdf = prefix_of(&prefixes, ns::RDF).to_owned();
        let prefix = prefix_of(&prefixes, ns);
        let mut property = Element::new(format!("{prefix}:{local}"));
        if resource {
            property.attributes_mut().push((format!("{rdf}:parseType"), "Resource".into()));
        }
        property.children = content(&prefixes, &rdf);
        insert(&mut description.children, Node::Element(property));
    }
}

/// One item of an ordered array, as [`Xmp::edit_seq`] hands it out: an `rdf:li` and the namespaces in scope on it.
///
/// Its fields are found wherever its structure puts them — as attributes or child elements of the `rdf:li`, of a
/// structure inside it (`Container:Item`), or of an `rdf:Description` inside that — so every form a writer may choose
/// reads and edits alike.
pub(crate) struct SeqItem<'a> {
    element: &'a mut Element,
    scope: Scope,
}

impl SeqItem<'_> {
    /// The decoded value of the field `ns:local`, the first found.
    pub(crate) fn get(&self, ns: &str, local: &str) -> Option<String> {
        field_value(self.element, &self.scope, ns, local, 0)
    }

    /// Sets the field `ns:local` to `value` wherever it is; where it is absent, adds it beside the field `beside` (in
    /// the same form, under the prefix the item binds to `ns` there). Answers whether it was set.
    pub(crate) fn set(&mut self, ns: &str, local: &str, value: &str, beside: (&str, &str)) -> bool {
        set_field(self.element, &self.scope, ns, local, value, 0)
            || add_field(self.element, &self.scope, beside, (ns, local), value, 0)
    }

    /// Removes the field `ns:local` wherever it is.
    pub(crate) fn remove(&mut self, ns: &str, local: &str) {
        remove_field(self.element, &self.scope, ns, local, 0);
    }
}

/// See [`SeqItem::get`]. `depth` bounds the walk, though the tree's own depth bound already does.
fn field_value(element: &Element, scope: &Scope, ns: &str, local: &str, depth: usize) -> Option<String> {
    if depth > tree::MAX_DEPTH {
        return None;
    }
    for (name, value) in element.attributes() {
        if scope.attribute(name) == Some((ns, local)) {
            return tree::decode(value).ok().map(|value| value.into_owned());
        }
    }
    element.elements().find_map(|child| {
        let inner = scope.with(child.attributes());
        if inner.element(&child.name) == Some((ns, local)) {
            simple_text(child)
        } else {
            field_value(child, &inner, ns, local, depth + 1)
        }
    })
}

/// See [`SeqItem::set`]: in place wherever the field is, answering whether it was found.
fn set_field(element: &mut Element, scope: &Scope, ns: &str, local: &str, value: &str, depth: usize) -> bool {
    if depth > tree::MAX_DEPTH {
        return false;
    }
    let mut found = element.attributes().iter().any(|(name, _)| scope.attribute(name) == Some((ns, local)));
    if found {
        for (name, old) in element.attributes_mut().iter_mut() {
            if scope.attribute(name) == Some((ns, local)) {
                *old = tree::encode(value);
            }
        }
    }
    for node in &mut element.children {
        let Node::Element(child) = node else { continue };
        let inner = scope.with(child.attributes());
        if inner.element(&child.name) == Some((ns, local)) {
            child.children = vec![Node::Text(tree::encode(value))];
            child.empty = false;
            found = true;
        } else {
            found |= set_field(child, &inner, ns, local, value, depth + 1);
        }
    }
    found
}

/// Adds `field` with `value` beside the first `anchor` found: as an attribute where the anchor is one, as an element
/// where it is one, under the prefix bound to the field's namespace there. Answers whether it was added.
fn add_field(
    element: &mut Element,
    scope: &Scope,
    anchor: (&str, &str),
    field: (&str, &str),
    value: &str,
    depth: usize,
) -> bool {
    if depth > tree::MAX_DEPTH {
        return false;
    }
    if element.attributes().iter().any(|(name, _)| scope.attribute(name) == Some(anchor)) {
        let Some(prefix) = scope.prefix_for(field.0) else { return false };
        let name = format!("{prefix}:{}", field.1);
        element.attributes_mut().push((name, tree::encode(value)));
        return true;
    }
    let anchored = element
        .elements()
        .any(|child| scope.with(child.attributes()).element(&child.name) == Some(anchor));
    if anchored {
        let Some(prefix) = scope.prefix_for(field.0) else { return false };
        let mut added = Element::new(format!("{prefix}:{}", field.1));
        added.children.push(Node::Text(tree::encode(value)));
        insert(&mut element.children, Node::Element(added));
        return true;
    }
    element.children.iter_mut().any(|node| match node {
        Node::Element(child) => {
            let inner = scope.with(child.attributes());
            add_field(child, &inner, anchor, field, value, depth + 1)
        }
        _ => false,
    })
}

/// See [`SeqItem::remove`].
fn remove_field(element: &mut Element, scope: &Scope, ns: &str, local: &str, depth: usize) {
    if depth > tree::MAX_DEPTH {
        return;
    }
    let attribute_matches = element.attributes().iter().any(|(name, _)| scope.attribute(name) == Some((ns, local)));
    if attribute_matches {
        element.attributes_mut().retain(|(name, _)| scope.attribute(name) != Some((ns, local)));
    }
    let mut kept: Vec<Node> = Vec::with_capacity(element.children.len());
    for node in std::mem::take(&mut element.children) {
        match node {
            Node::Element(child) if scope.with(child.attributes()).element(&child.name) == Some((ns, local)) => {
                if matches!(kept.last(), Some(Node::Text(text)) if text.trim().is_empty()) {
                    kept.pop();
                }
            }
            Node::Element(mut child) => {
                let inner = scope.with(child.attributes());
                remove_field(&mut child, &inner, ns, local, depth + 1);
                kept.push(Node::Element(child));
            }
            other => kept.push(other),
        }
    }
    element.children = kept;
}

/// The path to the first `rdf:RDF` element under `nodes`, and the scope inside it.
fn find_rdf(nodes: &[Node], scope: &Scope, path: &mut Vec<usize>) -> Option<(Vec<usize>, Scope)> {
    for (index, node) in nodes.iter().enumerate() {
        let Node::Element(element) = node else { continue };
        let inner = scope.with(element.attributes());
        path.push(index);
        if inner.element(&element.name) == Some((ns::RDF, "RDF")) {
            return Some((path.clone(), inner));
        }
        if let Some(found) = find_rdf(&element.children, &inner, path) {
            return Some(found);
        }
        path.pop();
    }
    None
}

/// Removes from each element under `nodes` its declarations of `ns` that nothing in their scope uses.
fn remove_unused_declarations(nodes: &mut [Node], ns: &str) {
    for node in nodes {
        let Node::Element(element) = node else { continue };
        let unused: Vec<String> = element
            .attributes()
            .iter()
            .filter_map(|(name, value)| {
                let prefix = name.strip_prefix("xmlns:")?;
                let declared = tree::decode(value).is_ok_and(|uri| uri == ns);
                (declared && !uses_prefix(element, prefix)).then(|| name.clone())
            })
            .collect();
        if !unused.is_empty() {
            element.attributes_mut().retain(|(name, _)| !unused.contains(name));
        }
        remove_unused_declarations(&mut element.children, ns);
    }
}

/// Whether `element` or anything inside it names an element or an attribute under `prefix` as `element` binds it (its
/// declarations aside, and nothing below a redeclaration of `prefix`).
fn uses_prefix(element: &Element, prefix: &str) -> bool {
    let under = |name: &str| name.strip_prefix(prefix).is_some_and(|rest| rest.starts_with(':'));
    let redeclares =
        |child: &Element| child.attributes().iter().any(|(name, _)| name.strip_prefix("xmlns:") == Some(prefix));
    under(&element.name)
        || element.attributes().iter().any(|(name, _)| under(name))
        || element
            .children
            .iter()
            .any(|node| matches!(node, Node::Element(child) if !redeclares(child) && uses_prefix(child, prefix)))
}

/// The text of an element holding only text, decoded.
fn simple_text(element: &Element) -> Option<String> {
    let mut text = String::new();
    for node in &element.children {
        match node {
            Node::Text(run) => text.push_str(run),
            Node::Markup(markup) => text.push_str(markup.strip_prefix("<![CDATA[")?.strip_suffix("]]>")?),
            Node::Element(_) => return None,
        }
    }
    tree::decode(&text).ok().map(|text| text.into_owned())
}

/// Makes sure each of `namespaces` has a prefix on `description` (declaring one where `scope` has none), and answers
/// `(URI, prefix)` for each.
fn declare_all(description: &mut Element, scope: &Scope, namespaces: &[&str]) -> Vec<(String, String)> {
    let mut scope = scope.clone();
    let mut prefixes = Vec::new();
    for uri in namespaces {
        if prefixes.iter().any(|(bound, _): &(String, String)| bound == uri) {
            continue;
        }
        let prefix = match scope.prefix_for(uri) {
            Some(prefix) => prefix.to_owned(),
            None => {
                let preferred = preferred_prefix(uri);
                let prefix = std::iter::once(preferred.to_owned())
                    .chain((1..).map(|n| format!("{preferred}{n}")))
                    .find(|candidate| scope.uri(candidate).is_none())
                    .expect("an unbound prefix exists");
                let declaration = (format!("xmlns:{prefix}"), tree::encode(uri));
                description.attributes_mut().push(declaration.clone());
                scope = scope.with(&[declaration]);
                prefix
            }
        };
        prefixes.push(((*uri).to_owned(), prefix));
    }
    prefixes
}

fn prefix_of<'a>(prefixes: &'a [(String, String)], uri: &str) -> &'a str {
    prefixes
        .iter()
        .find(|(bound, _)| bound == uri)
        .map(|(_, prefix)| prefix.as_str())
        .expect("declared")
}

/// The elements of a structure's fields.
fn structure(fields: &[Field<'_>], prefixes: &[(String, String)]) -> Vec<Node> {
    let mut nodes = Vec::new();
    for (uri, local, value) in fields {
        let mut field = Element::new(format!("{}:{local}", prefix_of(prefixes, uri)));
        field.children.push(Node::Text(tree::encode(value)));
        insert(&mut nodes, Node::Element(field));
    }
    nodes
}

/// The `rdf:Bag`, `rdf:Seq` or `rdf:Alt` of `items`.
fn array(kind: Array, items: &[String], rdf: &str) -> Vec<Node> {
    let container = match kind {
        Array::Bag => "Bag",
        Array::Seq => "Seq",
        Array::Alt => "Alt",
    };
    let mut array = Element::new(format!("{rdf}:{container}"));
    for item in items {
        let mut li = Element::new(format!("{rdf}:li"));
        if kind == Array::Alt {
            li.attributes_mut().push(("xml:lang".into(), "x-default".into()));
        }
        li.children.push(Node::Text(tree::encode(item)));
        insert(&mut array.children, Node::Element(li));
    }
    vec![Node::Element(array)]
}

/// Inserts `node` as the last child, ahead of any trailing indentation and on a line of its own.
fn insert(children: &mut Vec<Node>, node: Node) {
    let trailing = matches!(children.last(), Some(Node::Text(text)) if text.trim().is_empty());
    let at = if trailing { children.len() - 1 } else { children.len() };
    children.insert(at, node);
    children.insert(at, Node::Text("\n ".into()));
    if !trailing {
        children.push(Node::Text("\n".into()));
    }
}
