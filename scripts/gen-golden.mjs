// Renders every output of design/tui.js to tests/golden/ (ADR-002).
//
//   node scripts/gen-golden.mjs        (or `make golden`)
//
// Two builds: the default into tests/golden/, and ASCII glyphs with sixteen
// colours into tests/golden/ascii-16/. Each output becomes `name.txt` (the
// characters) and `name.ansi.txt` (each run of one style prefixed with
// `{fg [bg=x] [b] [u]}`); a flow becomes one pair per step, `name-0`,
// `name-1`. index.tsv lists every pair with its kind, size and tiers, so a
// test can tell which outputs exist without a table of its own.
//
// A truecolor run is named by its token. A sixteen-colour run is named by
// the colour the terminal is asked for, read from the design's own table:
// `dim` and `faint` are one colour there and `sel` is none, so a renderer's
// output cannot say which token it started from.

import { readFileSync, writeFileSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const out = join(root, 'tests/golden');

const sandbox = { window: {} };
vm.runInNewContext(readFileSync(join(root, 'design/tui.js'), 'utf8'), sandbox);
const TUI = sandbox.window.TUI;
const c16 = TUI.palette.c16;

function fail(message) { throw new Error(message); }

function style(run, colors) {
  if (colors !== '16') return { fg: run.fg, bg: run.bg, b: run.b, u: run.u };
  const fg = c16[run.fg] ?? fail(`no sixteen-colour value for ${run.fg}`);
  const bg = run.bg ? c16[run.bg] ?? fail(`no sixteen-colour value for ${run.bg}`) : null;
  return { fg, bg: bg === 'none' ? null : bg, b: run.b, u: run.u };
}

const tag = (s) => [s.fg, s.bg && `bg=${s.bg}`, s.b && 'b', s.u && 'u'].filter(Boolean).join(' ');

function render(rows, width, colors, label) {
  const plain = [];
  const styled = [];
  rows.forEach((row, y) => {
    const text = row.map((run) => run.t).join('');
    if ([...text].length !== width) fail(`${label}: row ${y} is ${[...text].length} cells, not ${width}`);
    let line = '';
    let last = null;
    for (const run of row) {
      const now = tag(style(run, colors));
      if (now !== last) line += `{${now}}`;
      line += run.t;
      last = now;
    }
    plain.push(text);
    styled.push(line);
  });
  return { plain: plain.join('\n') + '\n', styled: styled.join('\n') + '\n' };
}

function write(dir, colors, outputs) {
  const index = [];
  const one = (name, o) => {
    const tier = colors || o.colors;
    const { plain, styled } = render(o.rows, o.width, tier, name);
    writeFileSync(join(dir, `${name}.txt`), plain);
    writeFileSync(join(dir, `${name}.ansi.txt`), styled);
    index.push([name, o.kind, o.width, o.height, o.glyphs, tier].join('\t'));
  };
  for (const [name, o] of Object.entries(outputs)) {
    if (o.kind === 'screen') one(name, o);
    else if (o.kind === 'flow') o.steps.forEach((step, i) => one(`${name}-${i}`, step));
    else fail(`${name}: an output of kind ${o.kind}`);
  }
  writeFileSync(join(dir, 'index.tsv'), index.join('\n') + '\n');
  return index.length;
}

rmSync(out, { recursive: true, force: true });
mkdirSync(join(out, 'ascii-16'), { recursive: true });
const standard = write(out, null, TUI.build({}));
// build() has no switch for colours, so the tier is applied here. An
// output that is already ASCII in sixteen colours in the default build is
// the same file twice, and is written once.
const plainest = Object.fromEntries(Object.entries(TUI.build({ glyphs: 'ascii' }))
  .filter(([, o]) => !(o.kind === 'screen' && o.colors === '16')));
const ascii = write(join(out, 'ascii-16'), '16', plainest);
console.log(`wrote ${standard} outputs to tests/golden/ and ${ascii} to tests/golden/ascii-16/`);
