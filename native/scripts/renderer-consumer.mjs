#!/usr/bin/env node
// Execute the current host's actual consumer functions, extracted with its own
// TypeScript compiler. Inputs are real extracted-package JSONL results/calls.
// The only substitutes are UI setters and transport; no window or OS is opened.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { createRequire } from 'node:module';
import crypto from 'node:crypto';

const host = path.resolve(process.argv[2]);
const requireHost = createRequire(path.join(host, 'package.json'));
const ts = requireHost('typescript');
const hashes = {};
function read(relative) {
  const text = fs.readFileSync(path.join(host, relative), 'utf8');
  hashes[relative] = crypto.createHash('sha256').update(text).digest('hex');
  return text;
}
function find(source, predicate) {
  let matches = [];
  function visit(node) {
    if (predicate(node)) matches.push(node);
    ts.forEachChild(node, visit);
  }
  visit(source);
  assert.equal(matches.length, 1, 'actual host consumer must be unique');
  return matches[0];
}
const app = ts.createSourceFile('App.tsx', read('src/renderer/App.tsx'), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
const subscription = find(app, n => ts.isCallExpression(n) && n.expression.getText(app) === 'window.tezbar.onRunExtensionCommandFromHotkey');
const callback = subscription.arguments[0].getText(app);
const commandBar = ts.createSourceFile('CommandBar.tsx', read('src/renderer/CommandBar.tsx'), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
const launcher = find(commandBar, n => ts.isFunctionDeclaration(n) && n.name?.text === 'executeExtensionCommandViaRuntime').getText(commandBar);
function compile(source, environment) {
  const output = ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022, module:ts.ModuleKind.None}, reportDiagnostics:true});
  assert(!output.diagnostics?.some(d => d.category === ts.DiagnosticCategory.Error));
  const context = vm.createContext(environment);
  vm.runInContext(output.outputText, context);
  return context.extracted;
}
// Bind fixtures to the real host's notification request/reply and disposal
// semantics. These checks do not invoke the OS implementation or assume that a
// no-view result message becomes visible in the renderer.
const hostSource = read('src-tauri/src/native_core/extensions/process_host.rs');
const show = hostSource.slice(hostSource.indexOf('"host.notifications.show"=>{'), hostSource.indexOf('"host.notifications.remove"=>'));
for (const key of ['id','title','body']) assert(show.includes(`string(&params,"${key}")?`));
assert(show.includes('process_notifications::show(&scoped,title,body).await'));
assert(show.includes('Ok(json!(true))'));
assert(hostSource.includes('!r.notifications.is_empty()'));
const notifications = read('src-tauri/src/native_core/extensions/process_notifications.rs');
assert(notifications.includes('addNotificationRequest_withCompletionHandler'));
assert(notifications.includes("CreateToastNotifier('Tezbar').Show($toast)"));
const native = read('src-tauri/src/native_core/extensions/native.rs');
assert(native.includes('Err(message) => Some(Ok(json!({"ok":false,"message":message})))'));
assert(native.includes('if value["mode"] != "view"'));
assert(native.includes('p.dispose(&id).await'));
const processSource = read('src-tauri/src/native_core/extensions/process.rs');
assert(processSource.includes('&& !self.host.retained()'));
let input = '';
for await (const chunk of process.stdin) input += chunk;
const sessions = JSON.parse(input);
let successes = 0, failures = 0, silent = 0;
for (const session of sessions) {
  const result = session.frame.error ? {ok:false,message:session.frame.error.message} : session.frame.result;
  const expectedMessage = result.message;
  const terminal = session.reverse.filter(frame => frame.method === 'host.notifications.show');
  const tipsOff = [false, 'false'].includes(session.open.preferences?.showTips);
  assert.equal(terminal.length, !result.ok || !tipsOff ? 1 : 0);
  if (terminal.length) assert.deepEqual(terminal[0].params, {id:'paste-result',title:'Paste as Plain Text',body:expectedMessage});
  const effects = [], messages = [];
  const record = name => (...args) => effects.push([name,...args]);
  const window = {tezbar:{extensionRunCommand:async () => result, recordSearchActionUsage:async () => {}}};
  const hotkey = compile(`globalThis.extracted = (${callback});`, {window, setCommandInitialValue:record('value'),setExtensionRuntimeInitial:record('runtime'),setSurface:record('surface')});
  hotkey({extensionId:'com.tezbar.native.paste-as-plain-text',commandName:'paste-as-plain-text'});
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(effects.length, 0, 'current hotkey consumer ignores all no-view/error results');
  const command = compile(`${launcher}\nglobalThis.extracted = executeExtensionCommandViaRuntime;`, {
    window, showActionMsg:msg => messages.push(msg), clearPendingAction:record('clear'), setValue:record('value'),trackExtensionCommand:record('track'),onOpenExtensionRuntime:record('runtime'),focusCommandInput:record('focus'),formatExtensionRunError:String,
  });
  assert.equal(await command({extensionId:'com.tezbar.native.paste-as-plain-text',commandName:'paste-as-plain-text'}), result.ok);
  assert.deepEqual(messages, [expectedMessage]);
  assert(!effects.some(e => e[0] === 'runtime'));
  // Terminal notification is emitted BEFORE the session response is consumed,
  // so a hidden launcher / ignored hotkey result cannot suppress that request.
  if (!result.ok) failures++; else if (tipsOff) silent++; else successes++;
}
console.log(JSON.stringify({actualRendererConsumers:'passed', sessions:sessions.length, successes, failures, silent, hostSourceSha256:hashes}));
