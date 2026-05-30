#!/bin/bash
# Cria settings.json adaptado para Linux (sem hooks do PowerShell)
mkdir -p /home/vscode/.claude/hooks

cat > /home/vscode/.claude/settings.json << 'EOF'
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "\"$HOME/.claude/hooks/rtk-rewrite2.sh\""
          }
        ]
      }
    ]
  },
  "language": "portuguese",
  "effortLevel": "medium"
}
EOF

echo "Claude Code settings configured for Linux"
