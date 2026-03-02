Output "assets/downloading.gif"

Require "pahe-cli"

Set WindowBar Colorful
Set WindowBarSize 60
Set Shell "fish"
Set FontSize 20
Set Width 1200
Set Height 600

Type@25ms "pahe-cli download \" Enter
Type@10ms "--series https://animepahe.si/anime/4a9abc55-0a54-c544-3e14-736c79ddafe7 \" Enter
Type@25ms "--episodes 1"
Sleep 150ms
Enter

Sleep 20s
