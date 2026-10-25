#!/usr/bin/env python3
"""Genera logs sintéticos de Claude Code, Codex, Copilot, Cursor y Gemini para medir rendimiento.

Mismo formato que `tests/fixtures/<agente>/`, sin texto real (relleno de un vocabulario fijo) y
deterministas: con la misma semilla y tamaño salen archivos idénticos byte a byte.

Uso:
    python3 scripts/gen-fixtures.py 50m            # → src-tauri/target/perf-fixtures/50m/
    python3 scripts/gen-fixtures.py 1g --seed 3 --out /tmp/fx

Tamaños: `<n>k`, `<n>m` o `<n>g` (múltiplos de 1024). Desde 50m incluye además una sesión de
Claude Code de ~5 MiB (`sesion-grande-*.jsonl`) para el benchmark de ingesta de un archivo.

Estructura de salida (cada carpeta es la raíz de logs de su proveedor):
    claude_code/  codex/sessions/  copilot/session-state/  cursor/projects/  gemini/tmp/
"""

import argparse
import json
import random
import re
import shutil
import sys
import time
import uuid
from datetime import datetime, timedelta, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_OUT = REPO / "src-tauri" / "target" / "perf-fixtures"
MARKER = ".gen-fixtures"

UNITS = {"k": 1024, "m": 1024**2, "g": 1024**3}
BIG_SESSION_FROM = 50 * 1024**2
BIG_SESSION_BYTES = 5 * 1024**2

# Reparto del tamaño total, en orden de escritura.
SHARES = [("claude_code", 0.60), ("codex", 0.15), ("copilot", 0.10), ("cursor", 0.10), ("gemini", 0.05)]

WINDOW_END = datetime(2026, 9, 1, tzinfo=timezone.utc)
WINDOW = timedelta(days=90)
PROJECTS = [f"proyecto-{i:02d}" for i in range(20)]
BRANCHES = ["main", "develop", "feat/panel", "fix/ingesta", "perf/consultas"]

VOCAB = (
    "alfa beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho "
    "sigma tau upsilon phi chi psi omega lorem ipsum dolor sit amet consectetur adipiscing elit "
    "sed do eiusmod tempor incididunt ut labore et dolore magna aliqua enim ad minim veniam quis "
    "nostrud exercitation ullamco laboris nisi aliquip ex ea commodo consequat duis aute irure "
    "in reprehenderit voluptate velit esse cillum fugiat nulla pariatur excepteur sint occaecat "
    "cupidatat non proident sunt culpa qui officia deserunt mollit anim id est laborum"
).split()

SHELL_COMMANDS = [
    "cargo test 2>&1 | tail -20",
    "cargo build --release",
    "npm test",
    "npm run build",
    "git status",
    "git diff --stat",
    "ls -la src",
    "rg -n alfa src",
    "pytest -q",
    "make lint",
]
FILES = ["src/main.rs", "src/lib.rs", "src/app.ts", "src/views/panel.tsx", "README.md", "tests/alfa.rs"]
SKILLS = ["dataviz", "proper-testing", "brainstorming"]
SUBAGENT_TYPES = ["Explore", "Plan", "general-purpose"]
MCP_TOOLS = ["mcp__docs__read_page", "mcp__tracker__create_issue", "mcp__chat__send_message"]
CLAUDE_MODELS = ["claude-opus-5-5", "claude-sonnet-4-5", "claude-haiku-4-5"]
WEEKDAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]


def parse_size(raw: str) -> int:
    m = re.fullmatch(r"(\d+)([kmg])", raw.strip().lower())
    if not m or int(m.group(1)) == 0:
        raise ValueError(f"tamaño no válido: {raw!r} (usa <n>k, <n>m o <n>g)")
    return int(m.group(1)) * UNITS[m.group(2)]


def wants_big_session(size: int) -> bool:
    return size >= BIG_SESSION_FROM


class Gen:
    """Estado del generador: RNG, relleno de texto y contadores de ids."""

    def __init__(self, rng: random.Random):
        self.rng = rng
        words = [rng.choice(VOCAB) for _ in range(200_000)]
        self.pool = " ".join(words)
        self.counter = 0

    def text(self, lo: int, hi: int) -> str:
        n = self.rng.randint(lo, hi)
        start = self.rng.randrange(0, len(self.pool) - n - 1)
        return self.pool[start : start + n].strip() or "alfa"

    def uid(self) -> str:
        return str(uuid.UUID(int=self.rng.getrandbits(128), version=4))

    def hexid(self, n: int = 12) -> str:
        return f"{self.rng.getrandbits(n * 4):0{n}x}"

    def next_id(self, prefix: str) -> str:
        self.counter += 1
        return f"{prefix}{self.counter}"

    def start_time(self) -> datetime:
        secs = self.rng.uniform(0, WINDOW.total_seconds() - 6 * 3600)
        return WINDOW_END - WINDOW + timedelta(seconds=secs)

    def project(self) -> str:
        return self.rng.choice(PROJECTS)


def iso(t: datetime) -> str:
    return t.strftime("%Y-%m-%dT%H:%M:%S.") + f"{t.microsecond // 1000:03d}Z"


def ms(t: datetime) -> int:
    return int(t.timestamp() * 1000)


def cwd_of(project: str) -> str:
    return f"/home/dev/Proyectos/{project}"


def dashed(project: str) -> str:
    return cwd_of(project).replace("/", "-")


class Writer:
    """Escribe archivos JSONL y lleva la cuenta de bytes escritos."""

    def __init__(self):
        self.bytes = 0
        self.files = 0

    def write(self, path: Path, lines: list) -> int:
        data = "".join(json.dumps(obj, ensure_ascii=False) + "\n" for obj in lines).encode("utf-8")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        self.bytes += len(data)
        self.files += 1
        return len(data)


def line_size(obj) -> int:
    return len(json.dumps(obj, ensure_ascii=False).encode("utf-8")) + 1


# ---------------------------------------------------------------- Claude Code


def claude_usage(g: Gen) -> dict:
    write = g.rng.choice([0, 0, 0, g.rng.randint(100, 20000)])
    return {
        "input_tokens": g.rng.randint(1, 50),
        "cache_creation_input_tokens": write,
        "cache_read_input_tokens": g.rng.randint(5000, 150000),
        "output_tokens": g.rng.randint(20, 3000),
        "output_tokens_details": {"thinking_tokens": g.rng.randint(0, 500)},
        "cache_creation": {"ephemeral_1h_input_tokens": write, "ephemeral_5m_input_tokens": 0},
    }


def claude_tool(g: Gen, project: str):
    kind = g.rng.choices(
        ["Bash", "Read", "Edit", "Write", "Grep", "Agent", "Skill", "mcp"],
        weights=[30, 25, 15, 5, 10, 4, 3, 8],
    )[0]
    path = f"{cwd_of(project)}/{g.rng.choice(FILES)}"
    if kind == "Bash":
        return kind, {"command": g.rng.choice(SHELL_COMMANDS), "description": g.text(10, 40)}
    if kind == "Read":
        return kind, {"file_path": path}
    if kind == "Edit":
        return kind, {"file_path": path, "old_string": g.text(20, 400), "new_string": g.text(20, 400)}
    if kind == "Write":
        return kind, {"file_path": path, "content": g.text(200, 3000)}
    if kind == "Grep":
        return kind, {"pattern": g.rng.choice(VOCAB), "path": cwd_of(project)}
    if kind == "Agent":
        return kind, {"description": g.text(10, 40), "prompt": g.text(50, 400), "subagent_type": g.rng.choice(SUBAGENT_TYPES)}
    if kind == "Skill":
        return kind, {"skill": g.rng.choice(SKILLS)}
    return g.rng.choice(MCP_TOOLS), {"query": g.text(10, 80)}


class ClaudeSession:
    def __init__(self, g: Gen, root: Path, name: str | None = None):
        self.g = g
        self.project = g.project()
        self.session_id = g.uid()
        self.model = g.rng.choice(CLAUDE_MODELS)
        self.branch = g.rng.choice(BRANCHES)
        self.t = g.start_time()
        folder = root / dashed(self.project)
        self.path = folder / f"{name or self.session_id}.jsonl"
        self.sub_dir = folder / (name or self.session_id) / "subagents"
        self.lines = [{"type": "permission-mode", "permissionMode": "auto", "sessionId": self.session_id}]
        self.size = line_size(self.lines[0])
        self.subagents = []  # (path, lines)
        self.turn = 0

    def base(self, kind: str = "user", sidechain: bool = False) -> dict:
        return {
            "type": kind,
            "timestamp": iso(self.t),
            "cwd": cwd_of(self.project),
            "sessionId": self.session_id,
            "version": "2.1.282",
            "gitBranch": self.branch,
            "isSidechain": sidechain,
        }

    def tick(self, lo: float = 0.5, hi: float = 30.0):
        self.t += timedelta(seconds=self.g.rng.uniform(lo, hi))

    def push(self, obj, target=None):
        (self.lines if target is None else target).append(obj)
        self.size += line_size(obj)

    def assistant(self, content, sidechain=False, model=None, msg_id=None):
        return {
            **self.base("assistant", sidechain),
            "message": {
                "model": model or self.model,
                "id": msg_id or f"msg_{self.g.hexid(20)}",
                "type": "message",
                "role": "assistant",
                "content": content,
                "usage": claude_usage(self.g),
            },
        }

    def add_turn(self):
        g = self.g
        self.turn += 1
        prompt = f"p{self.turn}"
        self.tick(60, 1800)
        self.push({**self.base(), "promptId": prompt, "uuid": g.uid(), "message": {"role": "user", "content": g.text(30, 600)}})
        for _ in range(g.rng.randint(1, 6)):
            self.tick()
            tool, args = claude_tool(g, self.project)
            call_id = f"toolu_{g.hexid(20)}"
            msg_id = f"msg_{g.hexid(20)}"
            if g.rng.random() < 0.15:  # streaming: la misma respuesta llega dos veces
                self.push(self.assistant([{"type": "thinking", "thinking": "", "signature": g.hexid(16)}], msg_id=msg_id))
            self.push(self.assistant([{"type": "tool_use", "id": call_id, "name": tool, "input": args}], msg_id=msg_id))
            self.tick()
            result = {**self.base(), "promptId": prompt, "message": {"role": "user", "content": [
                {"type": "tool_result", "content": g.text(50, 4000), "is_error": g.rng.random() < 0.1, "tool_use_id": call_id}
            ]}}
            if tool == "Agent":
                agent_id = g.hexid(8)
                result["toolUseResult"] = {"agentId": agent_id, "status": "completed"}
                self.add_subagent(agent_id, prompt)
            self.push(result)
        self.tick()
        self.push(self.assistant([{"type": "text", "text": g.text(50, 1500)}]))

    def add_subagent(self, agent_id: str, prompt: str):
        g = self.g
        lines = [{**self.base("user", True), "promptId": prompt, "message": {"role": "user", "content": g.text(30, 300)}, "agentId": agent_id}]
        for _ in range(g.rng.randint(1, 4)):
            self.tick(0.2, 5)
            call_id = f"toolu_{g.hexid(20)}"
            a = self.assistant([{"type": "tool_use", "id": call_id, "name": g.rng.choice(["Grep", "Read"]), "input": {"pattern": g.rng.choice(VOCAB)}}], sidechain=True, model="claude-haiku-4-5")
            a["agentId"] = agent_id
            lines.append(a)
            lines.append({**self.base("user", True), "promptId": prompt, "message": {"role": "user", "content": [
                {"type": "tool_result", "content": g.text(50, 1500), "tool_use_id": call_id}
            ]}, "agentId": agent_id})
        self.size += sum(line_size(x) for x in lines)
        self.subagents.append((self.sub_dir / f"agent-{agent_id}.jsonl", lines))

    def write(self, w: Writer):
        w.write(self.path, self.lines)
        for path, lines in self.subagents:
            w.write(path, lines)


def write_big_session(root: Path, rng: random.Random) -> Path:
    """Sesión de Claude Code de ~5 MiB (sin subagentes, para medir la ingesta de un archivo)."""
    g = Gen(rng)
    s = ClaudeSession(g, root, name=f"sesion-grande-{g.uid()}")
    while s.size < BIG_SESSION_BYTES - 20_000:
        s.add_turn()
    s.subagents = []
    Writer().write(s.path, s.lines)
    return s.path


def claude_session(g: Gen, root: Path, budget: int, w: Writer):
    s = ClaudeSession(g, root)
    target = min(budget, g.rng.randint(30_000, 1_500_000))
    while s.size < target:
        s.add_turn()
    s.write(w)


# ---------------------------------------------------------------- Codex


def codex_session(g: Gen, root: Path, budget: int, w: Writer):
    project, t = g.project(), g.start_time()
    sid = g.uid()
    path = root / "sessions" / t.strftime("%Y/%m/%d") / f"rollout-{t.strftime('%Y-%m-%dT%H-%M-%S')}-{sid}.jsonl"
    cwd = cwd_of(project)
    lines = [{"timestamp": iso(t), "type": "session_meta", "payload": {
        "session_id": sid, "id": sid, "timestamp": iso(t), "cwd": cwd, "originator": "codex_cli_rs",
        "cli_version": "0.157.0", "source": "cli", "thread_source": "user",
        "git": {"commit_hash": g.hexid(40), "branch": g.rng.choice(BRANCHES), "repository_url": f"git@example.invalid:dev/{project}.git"}}}]
    size = line_size(lines[0])
    target = min(budget, g.rng.randint(20_000, 600_000))
    turn = 0

    def add(obj):
        nonlocal size
        lines.append(obj)
        size += line_size(obj)

    while size < target:
        turn += 1
        turn_id = f"turn-{turn}"
        t += timedelta(seconds=g.rng.uniform(60, 1800))
        add({"timestamp": iso(t), "type": "turn_context", "payload": {"turn_id": turn_id, "cwd": cwd, "approval_policy": "on-request", "sandbox_policy": {"type": "workspace-write"}, "model": "gpt-5-codex", "effort": "medium", "summary": "auto"}})
        prompt = g.text(30, 500)
        add({"timestamp": iso(t), "type": "event_msg", "payload": {"type": "user_message", "message": prompt, "images": []}})
        add({"timestamp": iso(t), "type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": prompt}]}})
        for _ in range(g.rng.randint(1, 5)):
            t += timedelta(seconds=g.rng.uniform(1, 20))
            call_id = f"call_{g.hexid(12)}"
            add({"timestamp": iso(t), "type": "response_item", "payload": {"type": "reasoning", "summary": [{"type": "summary_text", "text": g.text(20, 200)}], "content": None, "encrypted_content": g.text(200, 2000)}})
            if g.rng.random() < 0.7:
                args = json.dumps({"command": ["bash", "-lc", g.rng.choice(SHELL_COMMANDS)], "workdir": cwd})
                add({"timestamp": iso(t), "type": "response_item", "payload": {"type": "function_call", "name": "shell", "arguments": args, "call_id": call_id}})
            else:
                patch = f"*** Begin Patch\n*** Update File: {g.rng.choice(FILES)}\n@@\n-{g.text(20, 300)}\n+{g.text(20, 300)}\n*** End Patch"
                add({"timestamp": iso(t), "type": "response_item", "payload": {"type": "custom_tool_call", "status": "completed", "call_id": call_id, "name": "apply_patch", "input": patch}})
            inp = g.rng.randint(2000, 120000)
            usage = {"input_tokens": inp, "cached_input_tokens": g.rng.randint(0, inp), "cache_write_input_tokens": 0,
                     "output_tokens": g.rng.randint(20, 2000), "reasoning_output_tokens": g.rng.randint(0, 500), "total_tokens": inp}
            add({"timestamp": iso(t), "type": "token_usage_record", "payload": {"thread_id": sid, "turn_id": turn_id, "session_id": sid, "root_turn_id": turn_id, "response_id": f"resp_{g.hexid(16)}", "usage": usage, "turn_token_usage": usage, "thread_token_usage": usage}})
            out = json.dumps({"output": g.text(50, 3000), "metadata": {"exit_code": g.rng.choice([0, 0, 0, 1]), "duration_seconds": round(g.rng.uniform(0.1, 30), 1)}})
            add({"timestamp": iso(t), "type": "response_item", "payload": {"type": "function_call_output", "call_id": call_id, "output": out}})
        answer = g.text(50, 1200)
        add({"timestamp": iso(t), "type": "response_item", "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": answer}]}})
        add({"timestamp": iso(t), "type": "event_msg", "payload": {"type": "agent_message", "message": answer}})
        add({"timestamp": iso(t), "type": "event_msg", "payload": {"type": "task_complete", "last_agent_message": answer}})
    w.write(path, lines)


# ---------------------------------------------------------------- Copilot


def copilot_session(g: Gen, root: Path, budget: int, w: Writer):
    project, t = g.project(), g.start_time()
    sid = g.uid()
    models = ["claude-sonnet-4.5", "gpt-5.3-codex"]
    metrics = {m: {"requests": {"count": 0, "cost": 0}, "usage": {"inputTokens": 0, "outputTokens": 0, "cacheReadTokens": 0, "cacheWriteTokens": 0, "reasoningTokens": 0}} for m in models}
    lines = []
    size = 0

    def add(kind, data):
        nonlocal size
        obj = {"id": g.next_id("e"), "timestamp": ms(t), "type": kind, "data": data}
        lines.append(obj)
        size += line_size(obj)

    add("session.start", {"sessionId": sid, "copilotVersion": "1.0.88", "selectedModel": models[0], "startTime": iso(t), "context": {"cwd": cwd_of(project), "repository": f"dev/{project}", "branch": g.rng.choice(BRANCHES)}})
    target = min(budget, g.rng.randint(15_000, 400_000))
    turn = 0
    while size < target:
        turn += 1
        turn_id = f"turn{turn}"
        model = g.rng.choice(models)
        t += timedelta(seconds=g.rng.uniform(60, 1800))
        add("user.message", {"content": g.text(30, 500)})
        add("assistant.turn_start", {"turnId": turn_id})
        for _ in range(g.rng.randint(1, 4)):
            t += timedelta(seconds=g.rng.uniform(0.5, 20))
            call_id = f"call_{g.hexid(10)}"
            tool = g.rng.choice(["bash", "edit", "view"])
            args = {"command": g.rng.choice(SHELL_COMMANDS)} if tool == "bash" else {"path": g.rng.choice(FILES)}
            add("tool.execution_start", {"toolCallId": call_id, "toolName": tool, "turnId": turn_id, "arguments": args})
            t += timedelta(seconds=g.rng.uniform(0.2, 10))
            add("tool.execution_complete", {"toolCallId": call_id, "turnId": turn_id, "success": g.rng.random() > 0.1, "result": {"content": g.text(50, 2500)}})
            out = g.rng.randint(10, 1500)
            add("assistant.message", {"content": g.text(20, 800), "messageId": f"msg{g.hexid(10)}", "model": model, "outputTokens": out, "turnId": turn_id})
            mm = metrics[model]
            mm["requests"]["count"] += 1
            mm["requests"]["cost"] += 1
            mm["usage"]["inputTokens"] += g.rng.randint(500, 30000)
            mm["usage"]["outputTokens"] += out
            mm["usage"]["cacheReadTokens"] += g.rng.randint(0, 60000)
    t += timedelta(seconds=5)
    add("session.shutdown", {"currentModel": models[0], "modelMetrics": {m: v for m, v in metrics.items() if v["requests"]["count"]}})
    w.write(root / "session-state" / sid / "events.jsonl", lines)


# ---------------------------------------------------------------- Cursor


def cursor_stamp(t: datetime) -> str:
    local = t - timedelta(hours=7)
    hour = local.hour % 12 or 12
    ampm = "AM" if local.hour < 12 else "PM"
    return f"{WEEKDAYS[local.weekday()]}, {MONTHS[local.month - 1]} {local.day}, {local.year}, {hour}:{local.minute:02d} {ampm} (UTC-7)"


def cursor_session(g: Gen, root: Path, budget: int, w: Writer):
    project, t = g.project(), g.start_time()
    sid = g.uid()
    lines = []
    size = 0

    def add(obj):
        nonlocal size
        lines.append(obj)
        size += line_size(obj)

    target = min(budget, g.rng.randint(10_000, 300_000))
    while size < target:
        t += timedelta(seconds=g.rng.uniform(60, 1800))
        query = f"<user_info>OS: linux</user_info>\n<user_query>\n{g.text(30, 500)}\n</user_query>\n<timestamp>{cursor_stamp(t)}</timestamp>"
        add({"role": "user", "message": {"content": [{"type": "text", "text": query}]}})
        for _ in range(g.rng.randint(1, 4)):
            call_id = f"toolu_{g.hexid(10)}"
            tool = g.rng.choice(["shell", "edit_file", "read_file", "grep"])
            args = {"command": g.rng.choice(SHELL_COMMANDS)} if tool == "shell" else {"target_file": g.rng.choice(FILES), "instructions": g.text(10, 200)}
            add({"role": "assistant", "message": {"content": [{"type": "text", "text": g.text(20, 600)}, {"type": "tool_use", "id": call_id, "name": tool, "input": args}]}})
            add({"role": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": call_id, "content": g.text(50, 2500)}]}})
        add({"role": "assistant", "message": {"content": [{"type": "text", "text": g.text(50, 1200)}]}})
        add({"type": "turn_ended"})
    w.write(root / "projects" / dashed(project) / "agent-transcripts" / sid / f"{sid}.jsonl", lines)


# ---------------------------------------------------------------- Gemini


def gemini_session(g: Gen, root: Path, budget: int, w: Writer):
    project, t = g.project(), g.start_time()
    sid = g.uid()
    phash = f"{PROJECTS.index(project):04x}"
    lines = [
        {"sessionId": sid, "projectHash": phash, "startTime": iso(t), "lastUpdated": iso(t), "kind": "main"},
        {"$set": {"lastUpdated": iso(t), "directories": [cwd_of(project)]}},
    ]
    size = sum(line_size(x) for x in lines)
    target = min(budget, g.rng.randint(10_000, 300_000))
    while size < target:
        t += timedelta(seconds=g.rng.uniform(60, 1800))
        u = {"id": g.next_id("u"), "timestamp": iso(t), "type": "user", "content": [{"text": g.text(30, 500)}]}
        calls = []
        for _ in range(g.rng.randint(0, 3)):
            calls.append({"id": g.next_id("t"), "name": g.rng.choice(["run_shell_command", "replace", "read_file"]),
                          "args": {"command": g.rng.choice(SHELL_COMMANDS)}, "status": g.rng.choice(["success", "success", "error"]),
                          "timestamp": iso(t)})
        inp = g.rng.randint(1000, 80000)
        reply = {"id": g.next_id("g"), "timestamp": iso(t + timedelta(seconds=3)), "type": "gemini", "content": [{"text": g.text(50, 1500)}],
                 "model": g.rng.choice(["gemini-2.5-pro", "gemini-2.5-flash"]),
                 "tokens": {"input": inp, "output": g.rng.randint(20, 2000), "cached": g.rng.randint(0, inp), "thoughts": g.rng.randint(0, 300), "tool": 0, "total": inp}}
        if calls:
            reply["toolCalls"] = calls
        for obj in (u, reply):
            lines.append(obj)
            size += line_size(obj)
    name = f"session-{t.strftime('%Y-%m-%dT%H-%M')}-{sid[:8]}.jsonl"
    w.write(root / "tmp" / phash / "chats" / name, lines)


SESSION = {"claude_code": claude_session, "codex": codex_session, "copilot": copilot_session, "cursor": cursor_session, "gemini": gemini_session}


def generate(out: Path, size: int, seed: int) -> Writer:
    rng = random.Random(seed)
    g = Gen(rng)
    w = Writer()
    out.mkdir(parents=True, exist_ok=True)
    (out / MARKER).write_bytes(b"")
    if wants_big_session(size):
        w.bytes += write_big_session(out / "claude_code", rng).stat().st_size
        w.files += 1
    cumulative = 0.0
    for agent, share in SHARES:
        cumulative += share
        # Objetivo acumulado: lo que un agente se pasa se descuenta del siguiente.
        goal = int(size * cumulative)
        root = out / agent
        root.mkdir(parents=True, exist_ok=True)
        while goal - w.bytes > 2_000:
            SESSION[agent](g, root, goal - w.bytes, w)
    return w


def prepare(out: Path) -> None:
    if not out.exists():
        return
    if not (out / MARKER).exists() and any(out.iterdir()):
        raise SystemExit(f"{out} existe y no lo creó este generador: no se borra")
    shutil.rmtree(out)


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("size", help="tamaño objetivo: 50m, 200m, 1g o <n>{k,m,g}")
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--out", type=Path, help="carpeta de salida (por defecto src-tauri/target/perf-fixtures/<tamaño>)")
    args = ap.parse_args(argv)
    try:
        size = parse_size(args.size)
    except ValueError as e:
        ap.error(str(e))
    out = args.out or DEFAULT_OUT / args.size.lower()
    prepare(out)
    start = time.monotonic()
    w = generate(out, size, args.seed)
    print(f"{out}: {w.files} archivos, {w.bytes} bytes (objetivo {size}), {time.monotonic() - start:.1f} s", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
