# Record the README demo with VHS: mise run demo
Output demo/demo.gif

Require git

Set Shell "bash"
Set FontSize 20
Set Width 1280
Set Height 640
Set Padding 24
Set TypingSpeed 45ms
Set Theme "Catppuccin Mocha"

# Isolated setup: release binary on PATH, throwaway HOME (with a fake SSH config), profile store and repository.
Hide
Type `export PATH="$PWD/target/release:$PATH" HOME="$(mktemp -d)" PS1='\[\e[1;35m\]❯\[\e[0m\] ' && export G_CONFIG_DIR="$HOME/.config/g"`
Enter
Type `mkdir -p ~/.ssh && printf 'Host github-personal github-work\n  HostName github.com\n' > ~/.ssh/config`
Enter
Type `rm -rf /tmp/my-project && git init -q /tmp/my-project && cd /tmp/my-project && git remote add origin git@github.com:ada/my-project.git && g --version >/dev/null && clear`
Enter
# Only start recording once the setup has finished and the screen holds just the prompt.
Wait+Screen@20s /(?s)\A\s*❯\s*\z/
Sleep 300ms
Show

# Interactive add with the demand prompts
Type "g add work"
Sleep 300ms
Enter
Wait+Screen@10s /Git author name/
Sleep 500ms
Type "Ada Lovelace"
Sleep 300ms
Enter
Sleep 500ms
Type "ada@company.example"
Sleep 300ms
Enter
Sleep 700ms
Left
Sleep 700ms
Enter
Sleep 500ms
Type "~/.ssh/id_ed25519.pub"
Sleep 300ms
Enter
Sleep 800ms
Type "github-w"
Sleep 500ms
Tab
Sleep 1s
Enter
Sleep 1.5s

# Scriptable add
Type `g add personal --name "Ada Lovelace" --email ada@example.com`
Sleep 300ms
Enter
Sleep 1s

Type "g list"
Enter
Sleep 1s

# Apply and verify
Type "g use work"
Enter
Sleep 1s
Type "g info"
Enter
Sleep 1s
Type "git remote get-url origin"
Enter
Sleep 2s

Type "clear"
Enter
Sleep 300ms

# Terminal UI: switch to the personal profile
Type "g"
Enter
Wait+Screen@10s /Profiles/
Sleep 1s
Up
Sleep 700ms
Enter
Sleep 2s
Type "q"
Sleep 500ms

Type "git config user.email"
Enter
Sleep 2.5s

# Clean up the throwaway repository, HOME and profile store.
Hide
Type `cd / && rm -rf /tmp/my-project "$HOME"`
Enter
