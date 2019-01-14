# grup - offline markdown previewer

## What is grup?
```grup``` is an offline markdown previewer inspired by the impeccable [grip](https://github.com/joeyespo/grip) - minus sending your local files to [github](https://developer.github.com/v3/markdown/) for formatting.  
```grup``` serves the given markdown document by default at ```https://127.0.0.1:8000```.

## Example
Get preview for an .md file e.g. README.md:
```grup README.md```
To stop grup press Ctrl+C


## TODO
- [ ] Publish on [crates.io](https://crates.io)  
- [ ] Add github-like CSS (for [example](https://github.com/sindresorhus/github-markdown-css))  
- [ ] Add inotify hooks to refresh document (e.g. using a dedicated url /updates which returns 404 if there is nothing new; js' reload to refresh page)  

## License
[WTFPL - Do What the Fuck You Want to Public License 2](http://www.wtfpl.net)