#!/bin/sh
# Converts the mermaid `stateDiagram-v2` produced by `chart::render::to_mermaid`
# into PlantUML.
#
# Accepts either a raw diagram or a markdown file containing a ```mermaid fence;
# reads stdin when no file is given.
#
#   scripts/mermaid_to_plantuml.sh example/door_lock.md > example/door_lock.puml
#   cargo run && scripts/mermaid_to_plantuml.sh example/door_lock.md | plantuml -p > door_lock.png
#
# The two syntaxes line up almost exactly: `[*] --> X` and `A --> B` carry over
# unchanged, so only the header, the label separator and the line break differ.

set -eu

awk '
  { line[NR] = $0 }

  END {
      # Narrow to the mermaid fence when the input is markdown.
      first = 1
      last = NR
      for (i = 1; i <= NR; i++) {
          if (line[i] ~ /^[[:space:]]*```mermaid[[:space:]]*$/) {
              first = i + 1
              break
          }
      }
      if (first > 1) {
          for (i = first; i <= NR; i++) {
              if (line[i] ~ /^[[:space:]]*```[[:space:]]*$/) {
                  last = i - 1
                  break
              }
          }
      }

      print "@startuml"
      print "hide empty description"

      for (i = first; i <= last; i++) {
          s = line[i]
          sub(/^[[:space:]]+/, "", s)             # PlantUML uses no indentation
          if (s == "" || s == "stateDiagram-v2" || s ~ /^```/) continue

          # PlantUML prints the state name above its description, so drop the
          # copy that mermaid needs in the label.
          if (s ~ /^[A-Za-z0-9_]+ : /) {
              p = index(s, " : ")
              name = substr(s, 1, p - 1)
              desc = substr(s, p + 3)
              if (index(desc, name "<br/>") == 1) {
                  s = name " : " substr(desc, length(name) + 6)
              }
          }

          gsub(/<br\/>/, "\\n", s)                # line break inside a label
          sub(/ *: /, " : ", s)                   # PlantUML label separator
          print s
      }

      print "@enduml"
  }
' "$@"
