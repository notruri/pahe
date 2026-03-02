Output "assets/interactive.gif"

Require "pahe-cli"

Set WindowBar Colorful
Set WindowBarSize 60
Set Shell "fish"
Set FontSize 20
Set Width 1200
Set Height 600

# Entrypoint
Type@25ms "pahe-cli download --interactive"
Enter
Sleep 300ms

# Series URL
Type@25ms "https://animepahe.si/anime/4a9abc55-0a54-c544-3e14-736c79ddafe7" Enter
Sleep 1s

# Episodes
Backspace Type "6" Enter
Sleep 1s

# Quality
Down@150ms 2
Enter
Sleep 1s

# Audio Language
Enter
Sleep 1s

# Output
Sleep 20s
