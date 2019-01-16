# grup - offline markdown previewer

## What is grup?
```grup``` is an offline markdown previewer inspired by the impeccable [grip](https://github.com/joeyespo/grip) - minus sending your local files to [github](https://developer.github.com/v3/markdown/) for formatting.

## Example
Get preview for an .md file e.g. README.md:
```
grup README.md
```
Then click the displayed link to open the preview in your browser.

## TODO
- [ ] Publish on [crates.io](https://crates.io)
- [ ] Add inotify hooks + js to refresh document (e.g. using a dedicated url /updates which returns 404 if there is nothing new; else use js' document.reload() to refresh page)  

## Known Issues
### "No such remote or remote group: <filename>" when running grup
When trying to run grup in ```zsh```, the grup command will be shadowed by an alias defined by the zsh  ```git``` plugin.
This can be prevented by adding
```
unalias grup
```
to your ```.zshrc``` which removes the alias.

## Style
By default the html output is styled using [Github Markdown CSS by Sindre Sorhus](https://github.com/sindresorhus/github-markdown-css).

## License
[WTFPL - Do What the Fuck You Want to Public License 2](http://www.wtfpl.net)