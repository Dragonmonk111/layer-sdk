use std::pin::Pin;

use crate::prelude::*;
use futures::{pin_mut, Stream, StreamExt};
use futures_signals::{signal_vec, signal};
use layer_climb::querier::stream::BlockEvents;

pub struct BlockEventsUi {
    pub client: SigningClient,
    pub error: Mutable<Option<String>>,
    pub stream_ready: Mutable<bool>,
}

impl BlockEventsUi {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            client: CLIENT.get().unwrap_ext().clone(),
            error: Mutable::new(None),
            stream_ready: Mutable::new(false)
        })
    }

    pub fn render(&self) -> Dom {
        let stream = self.client.querier.clone().stream_block_events(None);

        html!("div", {
            .child_signal(signal::from_future(stream).map(|block_events| {
                match block_events {
                    Some(Ok(stream)) => {
                        Some(html!("div", {
                            .children_signal_vec(signal_vec::from_stream(stream).map(|block_events| {
                                match block_events {
                                    Ok(block_events) => {
                                        render_block_events(block_events)
                                    },
                                    Err(err) => {
                                        html!("div", {
                                            .class([&*TEXT_SIZE_MD, Color::Red.class()])
                                            .text("Error fetching block events")
                                        })
                                    }
                                }
                            }))
                        }))
                    },
                    Some(Err(err)) => {
                        Some(html!("div", {
                            .class([&*TEXT_SIZE_MD, Color::Red.class()])
                            .text("Error fetching block events")
                        }))
                    },
                    None => None
                }
            }))
        })
    }
}

pub fn render_block_events(block_events: BlockEvents) -> Dom {
    static CONTAINER:LazyLock<String> = LazyLock::new(|| {
        class! {
            .style("display", "flex")
            .style("flex-direction", "column")
            .style("gap", "1rem")
            .style("border", "1px solid black")
            .style("padding", "1rem")
        }
    });
    static HEADER:LazyLock<String> = LazyLock::new(|| {
        class! {
            .style("display", "flex")
            .style("gap", "10px")
            .style("justify-content", "space-between")
        }
    });
    let expanded = Mutable::new(false);

    let event_len = block_events.events.len();
    html!("div", {
        .class(&*CONTAINER)
        .child(html!("div", {
            .class(&*HEADER)
            .child(html!("div", {
                .text(&format!("Block #{}", block_events.height))
            }))
            .child(html!("div", {
                .class([&*Color::Accent.class(), &*CURSOR_POINTER])
                .text_signal(expanded.signal().map(move |is_expanded| {
                    if is_expanded {
                        format!("Hide events ({event_len})")
                    } else {
                        format!("Show events ({event_len})")
                    }
                }))
                .event(clone!(expanded => move |_: events::Click| {
                    expanded.set_neq(!expanded.get());
                }))
            }))
        }))
        .child(html!("div", {
            .style_signal("display", expanded.signal().map(|expanded| {
                if expanded {
                    "block"
                } else {
                    "none"
                }
            }))
            .children(block_events.events.iter().map(|event| {
                html!("div", {
                    .text(&format!("{:?}", event))
                })
            }))
        }))
    })
}