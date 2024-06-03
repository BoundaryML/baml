To install Ruby:

```
# this is it- Ruby is versioned by baml/.mise.toml
brew install mise
```


Run the tests like this:

```
(cd ../../engine/language_client_ruby && mise exec -- rake compile)

mise exec -- bundle install

infisical run --env=test -- mise exec -- rake test
```

If you want to just run `bundle` or `rake` directly, add mise shims to your shell ([mise docs](https://mise.jdx.dev/getting-started.html#_2a-activate-mise)):

```
echo 'eval "$(~/.local/bin/mise activate bash)"' >> ~/.bashrc
echo 'eval "$(~/.local/bin/mise activate zsh)"' >> ~/.zshrc
```