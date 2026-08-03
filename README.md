# koe

Turn plain English into shell commands.

```
$ koe how many files in my downloads folder
ls -1 ~/Downloads | wc -l
Counts the entries in ~/Downloads.
Execute? [y/N/e] y
      37
```

### Install

```
cargo install --path .
```

Then pick a backend.

**Gemini (default).** Get a [Gemini API key](https://aistudio.google.com/app/apikey) and export it — add this to `~/.zshrc` to make it permanent:

```
export GEMINI_API_KEY=your-api-key
```

**A local model.** Anything speaking the OpenAI chat-completions API works: Ollama, llama.cpp, `mlx_lm.server`, LM Studio. No API key, works offline, and nothing about your filesystem leaves the machine:

```
ollama pull qwen2.5-coder:7b
koe --provider openai count the files here
```

**OpenRouter or OpenAI.** Same flag, different endpoint:

```
export OPENROUTER_API_KEY=...
koe --provider openai --base-url https://openrouter.ai/api/v1 -m qwen/qwen-2.5-coder-32b-instruct <task>
```

### Shell integration

A command runs in a child process, so `cd` and `export` cannot outlive it on their own. Install the wrapper and they will:

```
eval "$(koe init zsh)"    # add to ~/.zshrc; also supports bash and fish
```

Without this, `koe make a rust project on the desktop` runs `cd ~/Desktop && cargo new my_project` and leaves you where you started.

### Safety

Every command is classified before you are asked about it, independently of what the model claims. Because the model is the thing being guarded against, the stricter of the two ratings wins.

| Level       | Meaning                                                  |
| ----------- | -------------------------------------------------------- |
| `safe`      | Read-only or trivially reversible                         |
| `caution`   | Writes, installs, or modifies state                       |
| `dangerous` | Destructive, irreversible, elevated, or system-wide       |

- `-y`, `--yes` — skip the prompt for `safe` commands only
- `--yolo` — skip the prompt at every level
- `--dry-run` — print the command without running it
- `e` at the prompt — rewrite the command before it runs

koe exits with the command's own exit code, and stderr is passed through, so `koe ... && next-thing` behaves the way you would expect. All prompts and spinners go to stderr, so `koe generate a secret | pbcopy` copies the output and nothing else.

### Configuration

Optional, at `~/.config/koe/config.toml`. Run `koe config` to see what is in effect.

```toml
provider = "gemini"                # or "openai"
model = "models/gemini-2.5-flash"
# base_url = "http://localhost:11434/v1"
auto_run = false                   # behave as if -y were always passed
json_mode = true                   # send response_format; disable for servers that reject it
log_history = true

[context]
git = true                         # branch name and whether the tree is dirty
files = true                       # directory listing and project type
tools = true                       # which CLI tools are installed
```

Everything under `[context]` is sent to the model with each request. Turn off whatever you would rather not share; a local backend keeps it on your machine regardless.

Add your own few-shot examples in `~/.config/koe/examples.jsonl` to teach koe your aliases and conventions:

```json
{"task":"deploy staging","response":{"kind":"command","command":"./scripts/deploy.sh --env staging","risk":"caution","explanation":"Runs the staging deploy script."}}
```

### History

Accepted, rejected, and rewritten commands are appended to `~/.local/state/koe/history.jsonl` (disable with `log_history = false`). Rejections and rewrites are the interesting part: they are labeled negative examples, which is what an eval set or a fine-tune needs and what a log of successful commands alone cannot provide.

### Todo

- [x] Safe and unsafe modes
- [x] Risk classification independent of the model
- [x] Switch models and providers (local, OpenRouter, OpenAI)
- [x] Custom context — project type, git state, installed tools
- [x] User-extensible examples for personal commands and workflows
- [x] Shell integration so `cd` persists
- [x] Ask for clarification instead of guessing
- [ ] REPL mode
- [ ] Interpret command output via `-i`
- [ ] Follow-ups that remember the previous command
- [ ] Caching for repeated tasks
- [ ] Fine-tuned local model built from the history log
