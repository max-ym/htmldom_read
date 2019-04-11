//!
//! # Examples
//!
//! To load nodes from HTML.
//! ```
//! # use htmldom_read::Node;
//! let html = r#"
//!     <div><p>Text</p></div>
//! "#;
//! // Load with default settings.
//! let nodes = Node::from_html(html, &Default::default()).unwrap().unwrap();
//! let first_node = nodes.children().get(0).unwrap();
//! // First node is <div>
//! assert_eq!("div", first_node.tag_name().unwrap());
//!
//! let children = first_node.children();
//!
//! // First child of <div> is <p>
//! let first_child = children.get(0).unwrap();
//! assert_eq!("p", first_child.tag_name().unwrap());
//! /// The child of <p> is Text
//! assert_eq!("Text", first_child.children().get(0).unwrap().text().unwrap());
//! ```
//!
//! Load node with text mixed with children. Text that is not mixed load inside the parent node and
//! not as separate child.
//! ```
//! # use htmldom_read::{Node, LoadSettings};
//! let html = r#"
//!     <p>Text <sup>child</sup> more text</p>
//! "#;
//! let settings = LoadSettings::new().all_text_separately(false);
//!
//! let from = Node::from_html(html, &settings).unwrap().unwrap();
//! let node = from.children().get(0).unwrap();
//! let children = node.children();
//!
//! let first_text = children.get(0).unwrap();
//! assert_eq!("Text ", first_text.text().unwrap());
//!
//! let sup = children.get(1).unwrap();
//! assert_eq!("child", sup.text().unwrap());
//!
//! let last_text = children.get(2).unwrap();
//! assert_eq!(" more text", last_text.text().unwrap());
//! ```

extern crate quick_xml;
extern crate memchr;

use quick_xml::events::{Event, BytesEnd, BytesText, BytesStart};
use quick_xml::{Error, Reader};
use std::collections::LinkedList;
use memchr::{memchr_iter};
use std::sync::{Arc};
use std::ops::{Deref, DerefMut};

type SharedNode = Arc<Node>;

/// Children of the node. All tags that are inside of parent node are listed in this struct.
#[derive(Default, Clone, Debug, PartialEq)]
pub struct Children(Vec<NodeAccess>);

/// How node is being stored and accessed.
#[derive(Debug, Clone)]
pub enum NodeAccess {
    Owned(Node),
    Sharable(SharedNode),
}

/// How children are stored in the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildrenType {
    Owned,
    Sharable,
}

/// Contains information about opening and corresponding closing tags. It also can
/// contain the value of the text between opening and closing tags if there are no children.
/// Otherwise, if there are children mixed with text then each text chunk is separated in
/// it's own node with other children in order they appear in the code.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Node {
    /// Start of the tag if any. It may be empty if this is a trailing text at the beginning of
    /// the HTML code. It also is empty in root node.
    start: Option<OpeningTag>,

    /// Text value if there is a text between opening and closing tags.
    text: Option<String>,

    /// Closing tag if any.
    end: Option<String>,

    /// Direct children of this node. Does not include children of children nodes.
    children: Children,
}

/// Information carried in the opening tag.
#[derive(Clone, Debug, PartialEq)]
pub struct OpeningTag {
    empty: bool, // Whether this tag is self-closing.
    name: String,
    attrs: Vec<Attribute>,
}

/// Attribute of the tag.
#[derive(Clone, Debug, PartialEq)]
pub struct Attribute {
    name: String,
    values: Vec<String>,
}

/// Settings that provide different options of how to parse HTML.
#[derive(Clone, PartialEq, Debug)]
pub struct LoadSettings {

    all_text_separately: bool,
    children_type: ChildrenType,
}

/// Settings to fetch children nodes that apply to given criteria.
///
/// # Examples
/// ```
/// # use htmldom_read::{ChildrenFetch, Node};
/// let html = r#"
/// <div id="mydiv">
///     <p class="someclass">Text</p>
/// </div>
/// <a class="someclass else">link</a>
/// "#;
///
/// // Create node tree for HTML code.
/// let node = Node::from_html(html, &Default::default()).unwrap().unwrap();
///
/// // Create criteria. Find all with `id='mydiv'`.
/// let fetch = node.children_fetch()
///         .key("id")
///         .value("mydiv");
///
/// // Search for all children that apply to criteria.
/// let result = fetch.fetch();
/// // Returns the first node: `<div id='mydiv'>`.
/// assert_eq!(result.iter().nth(0).unwrap(), &node.children().get(0).unwrap());
///
/// // Search for all with class='someclass' allowing it to contain other classes too.
/// let fetch = node.children_fetch()
///         .key("class")
///         .value_part("someclass");
/// let result = fetch.fetch();
/// // Returns the nodes <p> and <a>.
/// assert_eq!(result.iter().nth(0).unwrap(),
///         &node.children().get(0).unwrap().children().get(0).unwrap());
/// assert_eq!(result.iter().nth(1).unwrap(), &node.children().get(1).unwrap());
/// ```
#[derive(Clone, Copy)]
pub struct ChildrenFetch<'a> {
    /// Node to search in.
    node: &'a Node,

    /// Key to search for.
    key: Option<&'a str>,

    /// Exact value to search for.
    value: Option<&'a str>,

    /// If exact value is not set then this defines a part of the value separated with whitespaces
    /// to be found.
    value_part: Option<&'a str>,
}

/// Mutable `ChildrenFetch`. Allows to get mutable access to returned nodes.
pub struct ChildrenFetchMut<'a> {
    inner: ChildrenFetch<'a>,
}

impl IntoIterator for Children {

    type Item = NodeAccess;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Deref for Children {

    type Target = Vec<NodeAccess>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Children {

    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Children {

    fn iter_to_owned<T: IntoIterator<Item = Node>>(iter: T, capacity: usize) -> Children {
        let mut arr = Vec::with_capacity(capacity);
        for child in iter {
            arr.push(NodeAccess::new_owned(child));
        }

        Children(arr)
    }

    fn iter_to_shared<T: IntoIterator<Item = Node>>(iter: T, capacity: usize) -> Children {
        let mut arr = Vec::with_capacity(capacity);
        for child in iter {
            arr.push(NodeAccess::new_shared(child));
        }

        Children(arr)
    }

     fn iter_to<T: IntoIterator<Item = Node>>(children_type: &ChildrenType, iter: T, capacity: usize)
            -> Children {
        use ChildrenType::*;
        match children_type {
            Owned       => Children::iter_to_owned(iter, capacity),
            Sharable => Children::iter_to_shared(iter, capacity),
        }
    }

    /// Get sharable children by cloning data.
    pub fn to_sharable(&self) -> Self {
        let children = &self.0;
        let mut vec = Vec::with_capacity(children.len());
        for child in children {
            vec.push(child.to_sharable());
        }

        Children(vec)
    }

    /// Get owned children by cloning data.
    pub fn to_owned(&self) -> Self {
        let children = &self.0;
        let mut vec = Vec::with_capacity(children.len());
        for child in children {
            vec.push(child.to_owned());
        }

        Children(vec)
    }
}

impl PartialEq for NodeAccess {

    fn eq(&self, other: &NodeAccess) -> bool {
        use std::mem::discriminant;
        if discriminant(self) != discriminant(other) {
            return false;
        }

        use NodeAccess::*;
        match self {
            Owned(node) => {
                if let Owned(other) = other {
                    node == other
                } else {
                    unreachable!()
                }
            },
            Sharable(node) => {
                if let Sharable(other) = other {
                    Arc::ptr_eq(node, other)
                } else {
                    unreachable!()
                }
            },
        }
    }
}

impl Deref for NodeAccess {

    type Target = Node;

    fn deref(&self) -> &Node {
        use NodeAccess::*;
        match self {
            Owned(n) => n,
            Sharable(n) => n
        }
    }
}

impl NodeAccess {

    fn new_owned(node: Node) -> NodeAccess {
        NodeAccess::Owned(node)
    }

    fn new_shared(node: Node) -> NodeAccess {
        let arc = Arc::new(node);
        NodeAccess::Sharable(arc)
    }

    /// Try to access node mutably. If this node is owned then this is possible. For sharable nodes
    /// they can be accessed mutable only if they still were not shared.
    pub fn try_mut(&mut self) -> Option<&mut Node> {
        if let NodeAccess::Owned(n) = self {
            Some(n)
        } else if let NodeAccess::Sharable(n) = self {
            Arc::get_mut(n)
        } else {
            unreachable!()
        }
    }

    /// Convert this node to a sharable by cloning.
    pub fn to_sharable(&self) -> Self {
        use NodeAccess::*;
        match self {
            Owned(n) => Sharable(Arc::new(n.clone())),
            Sharable(n) => Sharable(n.clone())
        }
    }

    /// Convert this node to an owned by cloning.
    pub fn to_owned(&self) -> Self {
        use NodeAccess::*;
        match self {
            Owned(n) => Owned(n.clone()),
            Sharable(n) => Owned(n.as_ref().clone()),
        }
    }
}

impl Node {

    /// Create new empty node with no children nor tags.
    pub fn new() -> Self {
        Default::default()
    }

    /// Load node tree from HTML string.
    ///
    /// The root node has no start, end or text elements. It does have only children in it.
    /// When passing empty code, None will be returned.
    /// If there is an error parsing the HTML, then this function will fail and return the error
    /// type that occurred.
    pub fn from_html(html: &str, settings: &LoadSettings) -> Result<Option<Node>, Error> {
        use Event::*;
        use std::collections::linked_list::Iter;

        // Collect all events.
        let events = {
            let mut reader = Reader::from_str(html);
            let mut buf = Vec::new();
            let mut list = LinkedList::new();
            reader.check_end_names(false);
            loop {
                let event = {
                    match reader.read_event(&mut buf)? {
                        Start(e) => {
                            let vec = e.to_vec();
                            let e = BytesStart::borrowed(
                                &vec, e.name().len()
                            ).into_owned();
                            Some(Start(e))
                        },
                        End(e) => {
                            let vec = e.to_vec();
                            let e = BytesEnd::borrowed(&vec).into_owned();
                            Some(End(e))
                        },
                        Empty(e) => {
                            let vec = e.to_vec();
                            let e = BytesStart::borrowed(
                                &vec, e.name().len()
                            ).into_owned();
                            Some(Empty(e))
                        },
                        Text(e) => {
                            let vec = e.to_vec();
                            let e = BytesText::from_plain(&vec).into_owned();
                            Some(Text(e))
                        },
                        Eof => break,
                        _ => None,
                    }
                };

                if event.is_some() {
                    list.push_back(event.unwrap());
                }
            }

            // Remove trailing empty text on newlines.
            let fixed_list = {
                let trim_start = |s: String| {
                    if s.is_empty() {
                        return s;
                    }

                    let mut iter = s.chars();
                    let first = iter.next().unwrap();
                    if first == '\n' {
                        String::from(s.trim_start())
                    } else if first == '\t' || first == ' ' {
                        while let Some(ch) = iter.next() {
                            if ch != '\t' && ch != ' ' && ch != '\n' {
                                return s;
                            }
                        }
                        String::from(s.trim_start())
                    } else {
                        s
                    }
                };
                let trim_end = |s: String| {
                    let bytes = s.as_bytes();
                    let mut memchr = memchr_iter('\n' as _, bytes);
                    if let Some(_) = memchr.next() {
                        String::from(s.trim_end())
                    } else {
                        s
                    }
                };

                let mut fixed_list = LinkedList::new();
                for i in list {
                    if let Text(e) = i {
                        let text = std::str::from_utf8(e.escaped()).unwrap();
                        let text = String::from(text);
                        let s = trim_start(text);
                        let s = trim_end(s);
                        if !s.is_empty() {
                            let content = Vec::from(s.as_bytes());
                            let new = Text(BytesText::from_plain(&content)).into_owned();
                            fixed_list.push_back(new);
                        }
                    } else {
                        fixed_list.push_back(i);
                    }
                }
                fixed_list
            };

            fixed_list
        };

        // Function to read next node and it's children from event iterator.
        #[allow(unused_assignments)]
        fn next_node(iter: &mut Iter<Event>, settings: &LoadSettings) -> Option<Node> {
            let mut biter = iter.clone();
            let peek = biter.next();
            if peek.is_none() {
                return None;
            }
            let peek = peek.unwrap();
            match peek {
                Start(e) => {
                    iter.next(); // Confirm reading this event.

                    let start = Some({
                        let name = String::from(unsafe {
                            std::str::from_utf8_unchecked(
                            &*e.name()).split_whitespace().next().unwrap()
                        });

                        let mut attrs = LinkedList::new();
                        for attr in e.attributes() {
                            if let Err(_) = attr {
                                continue;
                            }
                            let attr = attr.unwrap();

                            let name = String::from(unsafe {
                                std::str::from_utf8_unchecked(attr.key)
                            });
                            let attr = Attribute::from_name_and_str_values(
                                name,
                                unsafe { std::str::from_utf8_unchecked(&*attr.value) }
                            );
                            attrs.push_back(attr);
                        }
                        let mut attrsvec = Vec::with_capacity(attrs.len());
                        for attr in attrs {
                            attrsvec.push(attr);
                        }

                        OpeningTag {
                            empty: false,
                            name,
                            attrs: attrsvec
                        }
                    });
                    let mut text = {
                        let peek = biter.next();
                        if let Some(peek) = peek {
                            match peek {
                                Text(e) => {
                                    iter.next(); // Confirm reading event.
                                    let s = unsafe { std::str::from_utf8_unchecked(e) };
                                    Some(String::from(s))
                                }
                                _ => {
                                    biter = iter.clone(); // Revert read.
                                    None
                                }
                            }
                        } else {
                            biter = iter.clone(); // Revert read.
                            None
                        }
                    };
                    let children = {
                        let mut children = LinkedList::new();
                        loop {
                            let child = next_node(iter, settings);
                            if let Some(child) = child {
                                children.push_back(child);
                            } else {
                                break;
                            }
                        }
                        biter = iter.clone(); // Apply changes of iter.

                        // Check whether to store text in separate node or in the same node.
                        // Text cannot be mixed with children as this will loose information about
                        // order of occurrences of children tags and the text values. So
                        // in this case all texts are saved as nodes on their own in children array.
                        // We only need to check already read text field as if it is read then it
                        // precedes any children nodes. All other texts are already on their own
                        // children nodes because of recursive call of this function.
                        if text.is_some() {
                            if !children.is_empty() || settings.all_text_separately {
                                // Store as separate node as first child as it actually is the first
                                // thing that was read.
                                children.push_front(Node {
                                    start: None,
                                    end: None,
                                    text,
                                    children: Default::default(),
                                });
                                text = None;
                            }
                        }

                        let len = children.len();
                        Children::iter_to(
                            &settings.children_type,
                            children,
                            len
                        )
                    };
                    let end = {
                        if start.is_some() { // Only opening tag can have a closing tag.
                            let peek = biter.next();
                            if peek.is_none() {
                                None
                            } else {
                                match peek.unwrap() {
                                    End(e) => {
                                        // Check if names are same. If not - discard and return None.
                                        if e.name() == start.as_ref().unwrap().name().as_bytes() {
                                            iter.next(); // Confirm reading end tag.
                                            let s = unsafe {
                                                std::str::from_utf8_unchecked(e.name())
                                            };
                                            Some(String::from(s))
                                        } else {
                                            biter = iter.clone();
                                            None
                                        }
                                    },
                                    _ => {
                                        biter = iter.clone();
                                        None
                                    }
                                }
                            }
                        } else {
                            None
                        }
                    };

                    let e = Some(Node {
                        start,
                        end,
                        text,
                        children,
                    });
                    e
                },
                Text(e) => {
                    iter.next();

                    Some(Node {
                        start: None,
                        end: None,
                        children: Default::default(),

                        text: Some(
                            String::from(unsafe { std::str::from_utf8_unchecked(&*e) })
                        ),
                    })
                },
                Empty(e) => {
                    iter.next();

                    let start = Some({
                        let name = e.name();
                        let name = String::from(unsafe {
                            std::str::from_utf8_unchecked(&*name)
                                .split_whitespace().next().unwrap()
                        });

                        OpeningTag {
                            empty: true,
                            name,
                            attrs: Default::default(),
                        }
                    });

                    Some(Node {
                        start,
                        end: None,
                        text: None,
                        children: Default::default(),
                    })
                },
                _ => None
            }
        }

        let children = {
            let mut nodes = LinkedList::new();
            let mut iter = events.iter();
            loop {
                let node = next_node(&mut iter, settings);
                if node.is_none() {
                    break;
                }
                nodes.push_back(node.unwrap());
            }

            let len = nodes.len();
            Children::iter_to(&settings.children_type, nodes.into_iter(), len)
        };

        if children.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Node {
                children,
                start: None,
                end: None,
                text: None,
            }))
        }
    }

    /// Start tag information.
    pub fn start(&self) -> &Option<OpeningTag> {
        &self.start
    }

    /// End tag information.
    pub fn end(&self) -> Option<&str> {
        if let Some(ref end) = self.end {
            Some(end)
        } else {
            None
        }
    }

    /// Text that appears between opening and closing tags.
    pub fn text(&self) -> Option<&str> {
        if let Some(ref s) = self.text {
            Some(s)
        } else {
            None
        }
    }

    /// Children tags of this node.
    pub fn children(&self) -> &Children {
        &self.children
    }

    /// The name of the tag that is represented by the node.
    pub fn tag_name(&self) -> Option<&str> {
        if let Some(ref start) = self.start {
            Some(&start.name)
        } else {
            None
        }
    }

    /// Start tag attributes.
    pub fn attributes(&self) -> Option<&Vec<Attribute>> {
        if let Some(ref start) = self.start {
            Some(&start.attrs)
        } else {
            None
        }
    }

    /// Find attribute by it's name.
    pub fn attribute_by_name(&self, key: &str) -> Option<&Attribute> {
        if let Some(ref start) = self.start {
            for attr in start.attributes() {
                if attr.name() == key {
                    return Some(attr);
                }
            }
        }
        None
    }

    /// Try saving given attribute in this node.
    ///
    /// # Failure
    /// If this attribute is already present then this function will not change it.
    /// If you need to overwrite the attribute anyway use [`overwrite_attribute`].
    pub fn put_attribute(&mut self, attr: Attribute) -> Result<(), Attribute> {
        if self.attribute_by_name(&attr.name).is_some() {
            Err(attr)
        } else {
            self.overwrite_attribute(attr);
            Ok(())
        }
    }

    /// Save this attribute in the node. If it is already present then overwrite it.
    pub fn overwrite_attribute(&mut self, attr: Attribute) {
        if self.start.is_none() {
            return;
        }

        // Find the attribute if it is present.
        let mut i = 0;
        let attrs = &mut self.start.as_mut().unwrap().attrs;
        while i < attrs.len() {
            let this = attrs.get_mut(i).unwrap();
            if attr.name == this.name {
                // Found. Overwrite.
                this.values = attr.values;
                return;
            }
            i += 1;
        }

        // Attribute was not found. Append new.
        attrs.push(attr);
    }

    /// Get children fetcher for this node to find children that apply to some criteria.
    pub fn children_fetch(&self) -> ChildrenFetch {
        ChildrenFetch::for_node(self)
    }

    pub fn children_fetch_mut(&mut self) -> ChildrenFetchMut {
        ChildrenFetchMut::for_node(self)
    }

    /// Convert this node and all it's children into HTML string.
    pub fn to_html(&self) -> String {
        let mut s = String::new();
        if let Some(name) = self.tag_name() {
            s += "<";
            s += &name;

            let attrs = &self.start.as_ref().unwrap().attrs;
            for attr in attrs {
                s += " ";
                s += &attr.name;
                s += "=\"";
                s += &attr.values_to_string();
                s += "\"";
            }

            if self.start.as_ref().unwrap().is_self_closing() {
                s += "/";
            }

            s += ">";
        }
        if let Some(ref text) = self.text {
            s += text;
        }

        for child in self.children.iter() {
            s += &child.to_html();
        }

        if let Some(ref end) = self.end {
            s += "</";
            s += end;
            s += ">";
        }

        s.shrink_to_fit();
        s
    }

    /// Change name of opening and closing tags (if any).
    pub fn change_name(&mut self, name: &str) {
        self.change_opening_name(name);
        self.change_closing_name(name);
    }

    /// Change the name of only opening tag if it exists.
    pub fn change_opening_name(&mut self, name: &str) {
        if let Some(ref mut start) = self.start {
            start.name = String::from(name);
        }
    }

    /// Change the name of only closing tag if it exists.
    pub fn change_closing_name(&mut self, name: &str) {
        if let Some(ref mut end) = self.end {
            *end = String::from(name);
        }
    }

    /// Mutable access to array of node's children.
    pub fn children_mut(&mut self) -> &mut Children {
        &mut self.children
    }
}

impl<'a> ChildrenFetch<'a> {

    /// Get children fetcher for given node to find children that apply to some criteria.
    pub fn for_node(node: &'a Node) -> Self {
        ChildrenFetch {
            node,
            key:        None,
            value:      None,
            value_part: None,
        }
    }

    /// Clone the fetcher with already set criteria but for given different node.
    pub fn same_for_node(&self, node: &'a Node) -> Self {
        let mut new = self.clone();
        new.node = node;
        new
    }

    /// Key to search for.
    pub fn key(mut self, key: &'a str) -> Self {
        self.key = Some(key);
        self
    }

    pub fn set_key(&mut self, key: &'a str) {
        self.key = Some(key);
    }

    /// Exact value to search for.
    pub fn value(mut self, value: &'a str) -> Self {
        self.value = Some(value);
        self
    }

    pub fn set_value(&mut self, value: &'a str) {
        self.value = Some(value);
    }

    /// If exact value is not set then this defines a part of the value separated with whitespaces
    /// to be found. If `value` is, however, set then this field is ignored entirely.
    pub fn value_part(mut self, part: &'a str) -> Self {
        self.value_part = Some(part);
        self
    }

    pub fn set_value_part(&mut self, part: &'a str) {
        self.value_part = Some(part);
    }

    /// Get all children and their children that apply to the criteria.
    pub fn fetch(self) -> LinkedList<&'a NodeAccess> {
        fn sub(criteria: ChildrenFetch) -> LinkedList<&NodeAccess> {
            let mut list = LinkedList::new();

            for child in criteria.node.children.iter() {
                let mut check_value_criteria = |attr: &Attribute| {
                    if let Some(value) = criteria.value {
                        if attr.values_to_string() == value {
                            list.push_back(child);
                        }
                    } else if let Some(part) = criteria.value_part {
                        let iter = attr.values().iter();
                        for i in iter {
                            if i == part {
                                list.push_back(child);
                                break;
                            }
                        }
                    } else {
                        // No value expected and finding of a key is enough.
                        list.push_back(child);
                    }
                };

                if let Some(key) = criteria.key {
                    if let Some(attr) = child.attribute_by_name(key) {
                        check_value_criteria(attr)
                    }
                } else {
                    let attrs = child.attributes().unwrap();
                    for attr in attrs {
                        check_value_criteria(attr)
                    }
                }

                let new_fetch = criteria.same_for_node(&child);
                let mut nodes = sub(new_fetch);
                list.append(&mut nodes);
            }

            list
        }

        sub(self)
    }
}

impl<'a> ChildrenFetchMut<'a> {

    /// Get children fetcher for given node to find children that apply to some criteria.
    pub fn for_node(node: &'a Node) -> Self {
        let inner = ChildrenFetch {
            node,
            key:        None,
            value:      None,
            value_part: None,
        };
        ChildrenFetchMut { inner }
    }

    /// Get all children and their children that apply to the criteria.
    pub fn fetch_mut(self) -> LinkedList<&'a mut NodeAccess> {
        let fetch = self.fetch();
        let mut result = LinkedList::new();
        for i in fetch {
            let a = i as *const NodeAccess as *mut NodeAccess;
            let a = unsafe { &mut *a };
            result.push_back(a);
        }
        result
    }

    pub fn fetch(self) -> LinkedList<&'a NodeAccess> {
        self.inner.fetch()
    }

    /// Clone the fetcher with already set criteria but for given different node.
    pub fn same_for_node(&self, node: &'a Node) -> Self {
        ChildrenFetchMut { inner: self.inner.same_for_node(node) }
    }

    /// Key to search for.
    pub fn key(self, key: &'a str) -> Self {
        let inner = self.inner.key(key);
        ChildrenFetchMut { inner }
    }

    /// Exact value to search for.
    pub fn value(self, value: &'a str) -> Self {
        let inner = self.inner.value(value);
        ChildrenFetchMut { inner }
    }

    /// If exact value is not set then this defines a part of the value separated with whitespaces
    /// to be found. If `value` is, however, set then this field is ignored entirely.
    pub fn value_part(self, part: &'a str) -> Self {
        let inner = self.inner.value_part(part);
        ChildrenFetchMut { inner }
    }
}

impl OpeningTag {

    /// Name of this tag.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Attributes of tag.
    pub fn attributes(&self) -> &Vec<Attribute> {
        &self.attrs
    }

    pub fn is_self_closing(&self) -> bool {
        self.empty
    }
}

impl Attribute {

    /// Create from a name and values passed as single string that are separated by whitespaces.
    pub fn from_name_and_str_values(name: String, values: &str) -> Self {
        let values = {
            let mut list = LinkedList::new();
            for val in values.split_whitespace() {
                list.push_back(String::from(val));
            }

            let mut vec = Vec::with_capacity(list.len());
            for val in list {
                vec.push(val);
            }

            vec
        };

        Attribute {
            name,
            values
        }
    }

    /// Create from a name and values passed as array of strings.
    /// They should not contain whitespaces and invalid characters for attributes or names.
    pub fn from_name_and_values(name: String, values: Vec<String>) -> Option<Self> {
        // TODO check on whitespaces.
        Some(Attribute {
            name,
            values
        })
    }

    /// The name of the attribute.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// All values stored in the attribute. Each value separated with whitespace is
    /// located in another string in the array. To get values as single string, use
    /// [`values_to_string`]
    pub fn values(&self) -> &Vec<String> {
        &self.values
    }

    /// Store all values in a string separated with spaces.
    pub fn values_to_string(&self) -> String {
        // Calculate the length of the string to allocate.
        let len = {
            let mut l = 0;
            for val in &self.values {
                l += val.len() + 1; // For space at the end.
            }
            l - 1 // Remove trailing last space.
        };

        let mut s = String::with_capacity(len);

        let mut i = 0;
        while i < self.values.len() {
            s += self.values.get(i).unwrap();

            i += 1;
            // Do not add last (trailing) space.
            if i < self.values.len() {
                s += " ";
            }
        }

        s
    }

    /// Get first value of the attribute if any.
    ///
    /// Usually, when attribute is known to contain single value this function makes it easier
    /// to obtain this value. It does not construct new string as `values_to_string` and
    /// is shorter than calling `values` and gettings first value manually (but is equivalent).
    ///
    /// # Panics
    /// This function will panic if there are no attribute values.
    pub fn first_value(&self) -> &String {
        self.values.get(0).unwrap()
    }

    /// Set new name for attribute.
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    /// Set new values for attribute. If any of passed strings have whitespaces then this
    /// function will fail.
    pub fn set_values(&mut self, values: Vec<String>) -> Result<(), ()> {
        // Check strings
        for s in &values {
            if s.split_whitespace().count() > 1 {
                return Err(());
            }
        }

        self.values = values;

        Ok(())
    }
}

impl Default for LoadSettings {

    fn default() -> Self {
        LoadSettings {
            all_text_separately: true,
            children_type: ChildrenType::Owned,
        }
    }
}

impl LoadSettings {

    pub fn new() -> Self {
        Default::default()
    }

    /// Store all text values in separate children nodes. Even those text which is alone
    /// in tag body without other children.
    ///
    /// True by default.
    pub fn all_text_separately(mut self, b: bool) -> Self {
        self.set_all_text_separately(b);
        self
    }

    /// See [`all_text_separately`].
    pub fn set_all_text_separately(&mut self, b: bool) {
        self.all_text_separately = b;
    }

    /// Node owns all of its children. This is a default value.
    pub fn owned_children(mut self) -> Self {
        self.children_type = ChildrenType::Owned;
        self
    }

    /// Node can share its children. Opposite to `owned_children`.
    pub fn sharable_children(mut self) -> Self {
        self.children_type = ChildrenType::Sharable;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_html() {
        let html = r#"
        <p>Some text
            <img src="a">
        </p>
        <a>Link</a>
        <br />
        "#;

        let result = Node::from_html(html, &Default::default());
        let result = result.unwrap();
        let root = result.unwrap();

        let node = root.children().get(0).unwrap();
        let start = node.start().as_ref();
        let name = start.unwrap().name();
        assert_eq!("p", name);

        let text = root.children().get(0).unwrap().children();
        let text = text.get(0).unwrap().text();
        assert_eq!("Some text", text.unwrap());

        let child = root.children().get(0).unwrap().children().get(1).unwrap();
        let child_name = child.tag_name();
        assert_eq!("img", child_name.unwrap());

        let child = root.children().get(1).unwrap();
        assert_eq!(child.tag_name().unwrap(), "a");
        assert_eq!("Link", child.children().get(0).unwrap().text().unwrap());

        let node = root.children().get(2).unwrap();
        assert_eq!("br", node.tag_name().unwrap());
    }

    #[test]
    fn from_html_separate_text() {
        let html = r#"
        <p>Text</p>
        "#;
        let load = Node::from_html(html, &LoadSettings::new()
            .all_text_separately(true));
        let load = load.unwrap().unwrap();

        let child = load.children().get(0).unwrap().children().get(0).unwrap();
        assert_eq!(child.text().unwrap(), "Text");
    }

    #[test]
    fn from_html_empty() {
        let html = "   ";

        let result = Node::from_html(html, &Default::default());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn from_html_with_spaces() {
        let html = "   <p>\n  Some  </p>";

        let result = Node::from_html(html, &Default::default());
        let result = result.unwrap().unwrap();

        let first = result.children().get(0).unwrap();
        assert_eq!(first.tag_name().unwrap(), "p");
        assert_eq!("Some  ", first.children().get(0).unwrap().text().unwrap());
    }

    #[test]
    fn node_to_html() {
        let html = "<p><i>Text</i><br></p>";

        let result = Node::from_html(html, &Default::default());
        let result = result.unwrap().unwrap();

        let new_html = result.to_html();

        assert_eq!(html, &new_html);
    }

    #[test]
    fn overwrite_attribute() {
        let html = "<a href='a'>";
        let result = Node::from_html(html, &Default::default());
        let mut result = result.unwrap().unwrap();
        let node = result.children_mut().get_mut(0).unwrap();

        let mut attr = node.attribute_by_name("href").unwrap().clone();
        attr.set_values(vec![String::from("b")]).unwrap();

        node.try_mut().unwrap().overwrite_attribute(attr);
        let html = result.to_html();

        assert_eq!("<a href=\"b\">", &html);
    }
}
