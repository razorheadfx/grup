# grup - offline markdown previewer
[![Latest Version](https://img.shields.io/crates/v/grup.svg)](https://crates.io/crates/grup)
[![Documentation](https://docs.rs/grup/badge.svg)](https://docs.rs/crate/grup)
[![License](https://img.shields.io/crates/l/grup.svg)](https://www.apache.org/licenses/LICENSE-2.0.html)
[![Build Status](https://travis-ci.org/razorheadfx/grup.svg?branch=master)](https://travis-ci.org/razorheadfx/grup)
[![dependency status](https://deps.rs/repo/github/razorheadfx/grup/status.svg)](https://deps.rs/repo/github/razorheadfx/grup)

## What is grup?
```grup``` is an offline markdown previewer inspired by the impeccable [grip](https://github.com/joeyespo/grip) - minus sending your local files to [github](https://developer.github.com/v3/markdown/) for formatting - therefore a little easier to stomach for privacy inclined people (like me).

## Installing
With [Rust installed](https://rustup.rs) run:
```shell
cargo install grup
``` 

## Usage
Get preview for an .md file e.g. README.md:
```shell
grup README.md
```
This will open a local webserver (by default at ```127.0.0.1:8000```) and display the rendered markdown.  
Refreshing the page will also cause the document to be updated.  
When you're done stop grup by pressing ```Ctrl+C```.  

## Contributors
Thanks to @NickeZ

## Known Issues
### "No such remote or remote group: <filename>" when running grup
When trying to run grup in ```zsh```, the grup command will be shadowed by an alias defined by the zsh  ```git``` plugin.
This can be prevented by adding
```shell
unalias grup
```
to your ```.zshrc``` which removes the alias.  
Alternatively: Add an alias pointing to the install location (e.g. ```alias grupp="~/.cargo/bin/grup"```)

## Style
By default the html output is styled using [Github Markdown CSS by Sindre Sorhus](https://github.com/sindresorhus/github-markdown-css).