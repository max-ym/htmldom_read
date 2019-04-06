# HTML reader

## Description
This library allows to read HTML strings and convert them to node tree.
With the tree it is easier to process data that is stored as HTML/XML file.
It is also possible to change the nodes and convert them back into HTML string.

## Current main features
- [x] Parsing attributes with spaces into multiple strings
- [x] Search for nodes that have particular attributes
- [] Change attributes
- [] Change tag name
- [] Create nodes manually
- [] Edit node's children array

## Examples
### To load nodes from HTML.
```
# use htmldom_read::Node;
let html = r#"
    <div><p>Text</p></div>
"#;
// Load with default settings.
let nodes = Node::from_html(html, &Default::default()).unwrap().unwrap();
let first_node = nodes.children().get(0).unwrap();
// First node is <div>
assert_eq!("div", first_node.tag_name().unwrap());

let children = first_node.children();

// First child of <div> is <p>
let first_child = children.get(0).unwrap();
assert_eq!("p", first_child.tag_name().unwrap());
/// The child of <p> is Text
assert_eq!("Text", first_child.children().get(0).unwrap().text().unwrap());
```

### Load node with text mixed with children.
 Text that is not mixed load inside the parent node and not as separate child.
```
# use htmldom_read::{Node, LoadSettings};
let html = r#"
    <p>Text <sup>child</sup> more text</p>
"#;
let settings = LoadSettings::new().all_text_separately(false);

let from = Node::from_html(html, &settings).unwrap().unwrap();
let node = from.children().get(0).unwrap();
let children = node.children();

let first_text = children.get(0).unwrap();
assert_eq!("Text ", first_text.text().unwrap());

let sup = children.get(1).unwrap();
assert_eq!("child", sup.text().unwrap());

let last_text = children.get(2).unwrap();
assert_eq!(" more text", last_text.text().unwrap());
```