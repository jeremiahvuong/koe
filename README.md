### Installation

Set the `GEMINI_API_KEY` environment variable to your [Gemini API key](https://aistudio.google.com/app/apikey).

```
export GEMINI_API_KEY=your-api-key
```

Recommended: install env globally via `~/.zshrc` or `~/.bashrc`. Run `open ~/.zshrc` and then add the export.

Install koe globally.

```
cargo install --path .
```

### Example Usage

| Input                                     | Output                                   |
| ----------------------------------------- | ---------------------------------------- |
| koe generate secret                       | openssl rand -base64 32                  |
| koe how many files in my downloads folder | ls -l ~/Downloads \| grep -v ^d \| wc -l |

### Todo

- [x] Un-safe and safe mode
- [ ] macros for common commands
- [ ] Have a way to add "context" i.e. if user has custom commands, or if user has a specific workflow
- [ ] Switch to different models (via openrouter for simplicity)
- [ ] REPL mode
- [ ] Interperate output of commands via `-i` flag
- [ ] "Interject" feature, allowing koe to ask for clarification or to do something else
- [ ] Previous commands -> `koe make new folder called "test"` -> `koe now add file "main.py" in there` koe should know that the file is in the folder
- [ ] Current directory state/info
- [ ] awarness of env variables
- [ ] Caching (if this is even feasible) maybe kv exacts
