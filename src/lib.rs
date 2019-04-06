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
//! let first_node = nodes.get(0).unwrap();
//! assert_eq!(first_node.tag_name().unwrap(), "div");
//!
//! let children = first_node.children();
//!
//! let first_child = children.get(0).unwrap();
//! assert_eq!(first_child.tag_name().unwrap(), "p");
//! assert_eq!(first_child.text().unwrap(), "Text");
//! ```
//!
//! Load node with text mixed with children.
//! ```
//! # use htmldom_read::Node;
//! let html = r#"
//!     <p>Text <sup>child</sup> more text</p>
//! "#;
//! let from = Node::from_html(html, &Default::default()).unwrap().unwrap();
//! let node = from.get(0).unwrap();
//! let children = node.children();
//!
//! let first_text = children.get(0).unwrap();
//! assert_eq!(first_text.text().unwrap(), "Text ");
//!
//! let sup = children.get(1).unwrap();
//! assert_eq!(sup.text().unwrap(), "child");
//!
//! let last_text = children.get(2).unwrap();
//! assert_eq!(last_text.text().unwrap(), " more text");
//! ```

pub extern crate quick_xml;
extern crate memchr;
use quick_xml::events::{Event, BytesEnd, BytesText, BytesStart};
use quick_xml::{Error, Reader};
use std::collections::LinkedList;
use memchr::{memchr3_iter, memchr_iter};
use quick_xml::events::attributes::{Attributes, Attribute};

/// Contains information about opening and corresponding closing tags. It also can
/// contain the value of the text between opening and closing tags if there are no children.
/// Otherwise, if there are children mixed with text then each text chunk is separated in
/// it's own node with other children in order they appear in the code.
#[derive(Clone, Debug)]
pub struct Node {

    // Static lifetime is ok because internally Event uses Cow<'a> as storage for a value.
    // When this Node is created ownership is converted from a reference with lifetime 'a into
    // owned value (without lifetime).

    /// Start of the tag if any. It may be empty if this is a trailing text at the beginning of
    /// the HTML code. It also is empty in root node.
    start: Option<BytesStart<'static>>,

    /// Text value if there is a text between opening and closing tags.
    text: Option<BytesText<'static>>,

    /// Closing tag if any.
    end: Option<BytesEnd<'static>>,

    /// Any direct children of this node. Does not include children of children nodes.
    children: Vec<Node>,
}

/// Settings that provide different options of how to parse HTML.
#[derive(Clone, PartialEq, Debug)]
pub struct LoadSettings {

    all_text_separately: bool,
}

impl Node {

    /// Load node tree from HTML string.
    ///
    /// The root node has no start, end or text elements. It does have only children in it.
    /// When passing empty code, None will be returned.
    /// If there is an error parsing the HTML, then this function will fail and return the error
    /// type that occurred.
    pub fn from_html(html: &str, settings: &LoadSettings) -> Result<Option<Vec<Node>>, Error> {
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
                            let e = BytesStart::borrowed_name(&vec).into_owned();
                            Some(Start(e))
                        },
                        End(e) => {
                            let vec = e.to_vec();
                            let e = BytesEnd::borrowed(&vec).into_owned();
                            Some(End(e))
                        },
                        Empty(e) => {
                            let vec = e.to_vec();
                            let e = BytesStart::borrowed_name(&vec).into_owned();
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

                    let start = Some(e.clone().into_owned());
                    let mut text = {
                        let peek = biter.next();
                        if let Some(peek) = peek {
                            match peek {
                                Text(e) => {
                                    iter.next(); // Confirm reading event.
                                    Some(e.clone().into_owned())
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

                        let mut vec = Vec::with_capacity(children.len());
                        for c in children {
                            vec.push(c);
                        }
                        vec
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
                                        if e.name() == start.as_ref().unwrap().name() {
                                            iter.next(); // Confirm reading end tag.
                                            Some(e.clone().into_owned())
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

                        text: Some(e.clone().into_owned()),
                    })
                },
                Empty(e) => {
                    iter.next();

                    Some(Node {
                        start: Some(e.clone().into_owned()),
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

            let mut vec = Vec::with_capacity(nodes.len());
            for n in nodes {
                vec.push(n);
            }
            vec
        };

        if children.is_empty() {
            Ok(None)
        } else {
            Ok(Some(children))
        }
    }

    /// Start tag information.
    pub fn start(&self) -> &Option<BytesStart<'static>> {
        &self.start
    }

    /// End tag information.
    pub fn end(&self) -> &Option<BytesEnd<'static>> {
        &self.end
    }

    /// Text that appears between opening and closing tags.
    pub fn text(&self) -> Option<&str> {
        if self.text.is_none() {
            return None;
        }

        Some(std::str::from_utf8(self.text.as_ref().unwrap().escaped()).unwrap())
    }

    /// Children tags of this node.
    pub fn children(&self) -> &Vec<Node> {
        &self.children
    }

    fn name_from_full(full: &[u8]) -> &str {
        // Locate the end of tag name.
        let end = {
            let mut memchr = memchr3_iter(
                ' ' as _,
                '\t' as _,
                '\n' as _,
                full);
            memchr.next()
        };

        if end.is_none() {
            std::str::from_utf8(full).unwrap()
        } else {
            std::str::from_utf8(&full[..end.unwrap()]).unwrap()
        }
    }

    /// The name of the tag that is represented by the node.
    pub fn tag_name(&self) -> Option<&str> {
        if self.start.is_none() {
            return None;
        }
        let start = self.start.as_ref().unwrap();

        Some(Self::name_from_full(start.name()))
    }

    /// Start tag attributes.
    pub fn attributes(&self) -> Option<Attributes> {
        if let Some(ref start) = self.start {
            Some(start.attributes())
        } else {
            None
        }
    }

    /// Find attribute by it's key.
    pub fn attribute_by_key(&self, key: &str) -> Option<Attribute> {
        if let Some(ref start) = self.start {
            for attr in start.attributes() {
                if let Ok(attr) = attr {
                    if attr.key == key.as_bytes() {
                        return Some(attr);
                    }
                } else {
                    // Looks like HTML code error!
                    return None;
                }
            }
        }
        None
    }
}

impl Default for LoadSettings {

    fn default() -> Self {
        LoadSettings {
            all_text_separately: false,
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
    /// False by default.
    pub fn all_text_separately(mut self, b: bool) -> Self {
        self.set_all_text_separately(b);
        self
    }

    /// See [`all_text_separately`].
    pub fn set_all_text_separately(&mut self, b: bool) {
        self.all_text_separately = b;
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

        let node = root.get(0).unwrap();
        let start = node.start().as_ref();
        let name = std::str::from_utf8(start.unwrap().name());
        assert_eq!(name.unwrap(), "p");

        let text = root.get(0).unwrap().children();
        let text = text.get(0).unwrap().text();
        assert_eq!(text.unwrap(), "Some text");

        let child = root.get(0).unwrap().children().get(1).unwrap();
        let child_name = child.tag_name();
        assert_eq!(child_name.unwrap(), "img");

        let child = root.get(1).unwrap();
        assert_eq!(child.tag_name().unwrap(), "a");
        assert_eq!(child.text().unwrap(), "Link");

        let node = root.get(2).unwrap();
        assert_eq!(node.tag_name().unwrap(), "br");
    }

    #[test]
    fn from_html_separate_text() {
        let html = r#"
        <p>Text</p>
        "#;
        let load = Node::from_html(html, &LoadSettings::new()
            .all_text_separately(true));
        let load = load.unwrap().unwrap();

        let child = load.get(0).unwrap().children().get(0).unwrap();
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

        let first = result.get(0).unwrap();
        assert_eq!(first.tag_name().unwrap(), "p");
        assert_eq!(first.text().unwrap(), "Some  ");
    }
}
