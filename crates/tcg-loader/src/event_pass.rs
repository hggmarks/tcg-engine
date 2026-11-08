//! The event pass: rejects what the parser options cannot, before any mapping to types.
//!
//! It reads the parser's event stream and rejects anchors, aliases and every tag (including the
//! core tags such as `!!str`, which `reject_unsupported_tags` lets through). It tracks the path of
//! the current node so that an error names the element at fault.

use serde_saphyr::granit_parser::{ErrorKind, Event, Marker, Parser};
use tcg_engine::definition_path::DefinitionPath;

use crate::error::{Location, YamlError, YamlErrorKind, YamlFeature};

/// One open collection, with the child being read.
enum Frame {
    Seq {
        current: usize,
    },
    Map {
        expect_key: bool,
        key: Option<String>,
    },
}

/// The path of the node that the next event belongs to.
fn current_path(stack: &[Frame]) -> DefinitionPath {
    let mut path = DefinitionPath::root();
    for frame in stack {
        match frame {
            Frame::Seq { current } => path = path.index(*current),
            Frame::Map {
                expect_key: false,
                key: Some(key),
            } => path = path.key(key.as_str()),
            Frame::Map { .. } => {}
        }
    }
    path
}

/// Records that the node at the top of the stack has been completely read.
fn finish_child(stack: &mut [Frame]) {
    match stack.last_mut() {
        Some(Frame::Seq { current }) => *current = current.saturating_add(1),
        Some(Frame::Map { expect_key, .. }) => *expect_key = !*expect_key,
        None => {}
    }
}

fn location_of(marker: &Marker) -> Location {
    Location {
        line: marker.line(),
        column: marker.col().saturating_add(1),
    }
}

fn forbidden(feature: YamlFeature, path: DefinitionPath, marker: &Marker) -> YamlError {
    YamlError {
        path,
        location: Some(location_of(marker)),
        kind: YamlErrorKind::ForbiddenFeature { feature },
    }
}

pub(crate) fn check(yaml: &str) -> Result<(), YamlError> {
    let mut stack: Vec<Frame> = Vec::new();
    for item in Parser::new_from_str(yaml) {
        let (event, span) = match item {
            Ok(item) => item,
            Err(error) => {
                let path = current_path(&stack);
                let marker = error.marker();
                // An alias to an anchor that was never defined stops the parser before it can
                // report the alias itself.
                if *error.kind() == ErrorKind::UnknownAnchor {
                    return Err(forbidden(YamlFeature::Alias, path, marker));
                }
                return Err(YamlError {
                    path,
                    location: Some(location_of(marker)),
                    kind: YamlErrorKind::Malformed {
                        message: error.info(),
                    },
                });
            }
        };
        match event {
            Event::Alias(_) => {
                return Err(forbidden(
                    YamlFeature::Alias,
                    current_path(&stack),
                    &span.start,
                ));
            }
            Event::Scalar(value, _, anchor, tag) => {
                reject_properties(anchor != 0, tag.is_some(), &stack, &span.start)?;
                if let Some(Frame::Map {
                    expect_key: true,
                    key,
                }) = stack.last_mut()
                {
                    *key = Some(value.to_string());
                }
                finish_child(&mut stack);
            }
            Event::SequenceStart(_, anchor, tag) => {
                reject_properties(anchor != 0, tag.is_some(), &stack, &span.start)?;
                stack.push(Frame::Seq { current: 0 });
            }
            Event::MappingStart(_, anchor, tag) => {
                reject_properties(anchor != 0, tag.is_some(), &stack, &span.start)?;
                stack.push(Frame::Map {
                    expect_key: true,
                    key: None,
                });
            }
            Event::SequenceEnd | Event::MappingEnd => {
                stack.pop();
                finish_child(&mut stack);
            }
            _ => {}
        }
    }
    Ok(())
}

fn reject_properties(
    anchored: bool,
    tagged: bool,
    stack: &[Frame],
    at: &Marker,
) -> Result<(), YamlError> {
    if anchored {
        return Err(forbidden(YamlFeature::Anchor, current_path(stack), at));
    }
    if tagged {
        return Err(forbidden(YamlFeature::Tag, current_path(stack), at));
    }
    Ok(())
}
