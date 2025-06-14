### Usage

Set the `GEMINI_API_KEY` environment variable to your Gemini API key.

```
export GEMINI_API_KEY=your-api-key
```

Install the binary.

```
cargo install --path .
```

### Todo

- [x] Un-safe and safe mode
- [ ] macros for common commands
- [ ] Have a way to add "context" i.e. if user has custom commands, or if user has a specific workflow
- [ ] Switch to different models (via openrouter for simplicity)
- [ ] REPL mode
- [ ] Interperate output of commands via -i flag
- [ ] "Interject" feature, allowing koe to ask for clarification or to do something else
- [ ] Previous commands -> koe make new folder called "test" -> koe now add file "main.py" koe should know that the file is in the folder
- [ ] Current directory state/info
- [ ] awarness of env variables
- [ ] Caching (if this is even feasible) maybe kv exacts
