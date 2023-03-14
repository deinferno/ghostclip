use std::error::Error;
use std::sync::Mutex;

use clap::Parser;

use once_cell::sync::OnceCell;

use x11rb::connection::Connection;
use x11rb::protocol::xfixes::{ConnectionExt as XFixesConnectionExt, SelectionEventMask};
use x11rb::protocol::xproto::{
    Atom, ConnectionExt, CreateWindowAux, EventMask, GetPropertyType, PropMode,
    SelectionNotifyEvent, SelectionRequestEvent, WindowClass, SELECTION_NOTIFY_EVENT,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::CURRENT_TIME;

/// Simple program to preserve clipboard text after window is closed
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Forcefully grab ownership of clipboard data (doesn't require Xfixes)
    #[arg(short, long, default_value_t = false)]
    intrusive: bool,
}

static INTRUSIVE: OnceCell<bool> = OnceCell::new();
static INCR: OnceCell<Atom> = OnceCell::new();
static CLIPBOARD: OnceCell<Atom> = OnceCell::new();
static UTF8_STRING: OnceCell<Atom> = OnceCell::new();
static GHOSTCLIP_PROPERTY: OnceCell<Atom> = OnceCell::new();

static DATA: Mutex<Vec<u8>> = Mutex::new(Vec::new());

fn grab(conn: &RustConnection, win_id: u32, time: u32) -> Result<(), Box<dyn Error>> {
    if conn.get_selection_owner(*CLIPBOARD.wait())?.reply()?.owner == x11rb::NONE {
        println!("Claiming ownership of unowned clipboard");
        conn.set_selection_owner(win_id, *CLIPBOARD.wait(), time)?
            .check()?;
        return Ok(());
    }

    conn.convert_selection(
        win_id,
        *CLIPBOARD.wait(),
        *UTF8_STRING.wait(),
        *GHOSTCLIP_PROPERTY.wait(),
        time,
    )?
    .check()?;

    conn.flush()?;

    let event = conn.wait_for_event()?;
    let mut event_option = Some(event);
    while let Some(event) = event_option {
        match event {
            Event::SelectionNotify(event) => {
                if event.property != x11rb::NONE {
                    let property = conn
                        .get_property(
                            false,
                            win_id,
                            *GHOSTCLIP_PROPERTY.wait(),
                            GetPropertyType::ANY,
                            0,
                            u32::MAX,
                        )?
                        .reply()?;

                    if property.type_ == *INCR.wait() {
                        println!("INCR handling not implemented");
                        conn.flush()?;
                        return Ok(());
                    }

                    println!("Copying clipboard text");

                    *DATA.lock()? = property.value;

                    if *INTRUSIVE.wait() {
                        println!("taking ownership of clipboard");
                        conn.delete_property(win_id, *GHOSTCLIP_PROPERTY.wait())?
                            .check()?;
                        conn.set_selection_owner(win_id, *CLIPBOARD.wait(), time)?
                            .check()?;
                    }

                    conn.flush()?;
                }
            }
            _ => {
                conn.flush()?;
            }
        }
        event_option = conn.poll_for_event()?;
    }

    return Ok(());
}

fn deny(conn: &RustConnection, event: &SelectionRequestEvent) -> Result<(), Box<dyn Error>> {
    let fevent = SelectionNotifyEvent {
        response_type: SELECTION_NOTIFY_EVENT,
        requestor: event.requestor,
        selection: event.selection,
        target: event.target,
        property: x11rb::NONE,
        time: event.time,
        ..Default::default()
    };

    conn.send_event(true, event.requestor, EventMask::NO_EVENT, fevent)?
        .check()?;

    conn.flush()?;

    Ok(())
}

fn fulfill(conn: &RustConnection, event: &SelectionRequestEvent) -> Result<(), Box<dyn Error>> {
    let data = DATA.lock()?.clone();

    conn.change_property(
        PropMode::REPLACE,
        event.requestor,
        event.property,
        *UTF8_STRING.wait(),
        8,
        data.len() as u32,
        &data,
    )?
    .check()?;

    let fevent = SelectionNotifyEvent {
        response_type: SELECTION_NOTIFY_EVENT,
        requestor: event.requestor,
        selection: event.selection,
        target: event.target,
        property: event.property,
        time: event.time,
        ..Default::default()
    };

    conn.send_event(true, event.requestor, EventMask::NO_EVENT, fevent)?
        .check()?;

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    INTRUSIVE.set(args.intrusive).unwrap();

    let (conn, screen_num) = x11rb::connect(None)?;

    INCR.set(conn.intern_atom(false, b"INCR")?.reply()?.atom)
        .unwrap();
    CLIPBOARD
        .set(conn.intern_atom(false, b"CLIPBOARD")?.reply()?.atom)
        .unwrap();
    UTF8_STRING
        .set(conn.intern_atom(false, b"UTF8_STRING")?.reply()?.atom)
        .unwrap();
    GHOSTCLIP_PROPERTY
        .set(conn.intern_atom(false, b"GHOSTCLIP")?.reply()?.atom)
        .unwrap();

    let screen = &conn.setup().roots[screen_num];

    let win_id = conn.generate_id()?;

    let win_aux = CreateWindowAux::new();

    conn.create_window(
        screen.root_depth,
        win_id,
        screen.root,
        -10,
        -10,
        1,
        1,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &win_aux,
    )?
    .check()?;

    if !*INTRUSIVE.wait() {
        conn.query_extension(b"XFIXES")?.reply()?;
        conn.xfixes_query_version(5, 0)?.reply()?;
        conn.xfixes_select_selection_input(
            win_id,
            *CLIPBOARD.wait(),
            SelectionEventMask::SET_SELECTION_OWNER
                | SelectionEventMask::SELECTION_WINDOW_DESTROY
                | SelectionEventMask::SELECTION_CLIENT_CLOSE,
        )?
        .check()?;
        conn.flush()?;
    }

    grab(&conn, win_id, CURRENT_TIME)?;

    loop {
        let event = conn.wait_for_event()?;
        let mut event_option = Some(event);
        while let Some(event) = event_option {
            match event {
                Event::XfixesSelectionNotify(event) if !*INTRUSIVE.wait() => {
                    println!("Handling Xfixes SelectionNotify");
                    grab(&conn, win_id, event.timestamp)?;
                }
                Event::SelectionNotify(event) => {
                    println!("Handling SelectionNotify");
                    grab(&conn, win_id, event.time)?;
                }
                Event::SelectionClear(event) => {
                    println!("Handling selection clear");
                    grab(&conn, win_id, event.time)?;
                }
                Event::SelectionRequest(event) => {
                    if DATA.lock()?.is_empty()
                        || event.target != *UTF8_STRING.wait()
                        || event.property == x11rb::NONE
                    {
                        deny(&conn, &event)?;
                    } else {
                        println!("Providing clipboard text");
                        fulfill(&conn, &event)?;
                    }
                }
                _ => {
                    conn.flush()?;
                }
            }

            event_option = conn.poll_for_event()?;
        }
    }
}
