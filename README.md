ghostclip - A Simple Clipboard Proxy for X11
================================================

This is simple program that watches for clipboard text and proxies it for
cases where original selection's window is closed.

To keep program simple it only handles text clipboard.

To avoid breaking stuff when it's proxying capability are not required
uses passive mode via Xfixes.

## Problem

Most clip managers got too much features and dependencies or don't handle clipboard proxying.

So after finding `xclipd` it was doing most of things i need but it was too intrusive
and broke other clipboard mimes, so i decided to "meme" rewrite it to rust and use xfixes selection events.

## How it works

Almost the same as `xclipd` but doesn't take ownership of clipboard if original owner is still around.

## Building

You will need to setup rust toolchain, try https://rustup.rs/

    $ git clone https://github.com/deinferno/ghostclip/
    $ cd ghostclip
    $ cargo build --release
    $ sudo cp target/release/ghostclip /usr/local/bin

## More Information

Based on implementation: https://github.com/jhunt/xclipd/

And documentation in blog post: https://jameshunt.us/writings/x11-clipboard-management-foibles.html