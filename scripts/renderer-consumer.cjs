// Executes the actual current renderer callback/filter from read-only host source.
// No browser, module application imports, clipboard or GUI.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const host = process.env.TEZBAR_HOST_SOURCE || path.resolve(__dirname, '../../../Tezbar');
const ts = require(path.join(host, 'node_modules/typescript'));
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const source = fs.readFileSync(path.join(host, 'src/renderer/ExtensionRuntimeView.tsx'), 'utf8');
const tree = ts.createSourceFile('view.tsx', source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
let callback;
function visit(node) {
  if (ts.isJsxAttribute(node) && node.name.getText(tree) === 'onInvokeAction' && node.initializer?.expression && ts.isArrowFunction(node.initializer.expression)) {
    const expression = node.initializer.expression.getText(tree);
    if (expression.includes('extensionInvokeAction')) callback = expression;
  }
  ts.forEachChild(node, visit);
}
visit(tree);
assert(callback, 'actual onInvokeAction callback not found');
// native.rs::decorate attaches routing metadata only to full view replies.
const native = fs.readFileSync(path.join(host,'src-tauri/src/native_core/extensions/native.rs'),'utf8');
assert(native.includes('Some("view") => {') && native.includes('value["sessionId"] = json!(id);'));
for (const value of [input.before,input.response]) {
  if(value.mode==='view') Object.assign(value,{sessionId:'renderer-test',extensionId:'com.tezbar.native.convert-px-to-vw-vh',commandName:input.before.root.type==='Form'?'index':'menu-bar',generation:1,refreshMs:null});
}
let next = input.before;
const owner = {};
const feedbackSource = fs.readFileSync(path.join(host, 'src/renderer/extensionFeedback.ts'), 'utf8');
const feedbackExports = {};
vm.runInNewContext(ts.transpileModule(feedbackSource, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText, {exports:feedbackExports});
const sandbox = {
  extensionNotice: feedbackExports.extensionNotice,
  state: input.before, isFormSurface: input.before.root.type === 'Form',
  pendingSubmitRef: {current:null}, currentSession: {current:owner}, sessionOwner:owner,
  setError: value => { if(value) throw Error(value); }, showActionFailure: value => {throw Error(value)},
  showActionNotice: () => null, setActionNotice: () => {},
  effectNotice: () => null, fallbackActionNotice: () => null,
  setState: value => { next = typeof value === 'function' ? value(next) : value; },
  window: {tezbar:{extensionInvokeAction:async () => input.response}},
};
(async () => {
  const js = ts.transpileModule(`const invoke = (${callback}); invoke;`, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
  await vm.runInNewContext(js, sandbox)(input.actionId, {value:'1'});
  assert.equal(next.root.type, input.before.root.type, 'next render dereferences root.type');
  assert(next.actions && next.sessionId, 'complete renderer state');
  if (next.root.type === 'Form') {
    const formSource = fs.readFileSync(path.join(host,'src/renderer/src/raycast-api/form-runtime.tsx'),'utf8');
    const ft = ts.createSourceFile('form.tsx',formSource,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);
    const names = new Set(['collectOptionData','collectFormItems','sameFormValue','reconcileFormValues']);
    const functions = ft.statements.filter(n=>ts.isFunctionDeclaration(n)&&names.has(n.name?.text)).map(n=>n.getText(ft).replace(/^export /,''));
    assert.equal(functions.length,names.size);
    const program = functions.join('\n') + '\n({collectFormItems,reconcileFormValues});';
    const js = ts.transpileModule(program,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
    const form=vm.runInNewContext(js,{});
    const fields=root=>form.collectFormItems(root).filter(i=>i.type==='field').map(i=>i.field);
    const previousFields=fields(input.before.root), nextFields=fields(next.root);
    const local={...form.reconcileFormValues(previousFields),textfield:'123 queued locally'};
    const reconciled=form.reconcileFormValues(nextFields,local,previousFields);
    assert.equal(reconciled.textfield,local.textfield,'current form renderer must preserve queued typing');
  }
  if (input.row) {
    const surface = fs.readFileSync(path.join(host,'src/renderer/src/raycast-api/index.tsx'),'utf8');
    const st = ts.createSourceFile('surface.tsx',surface,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);
    let filter;
    function scan(node) {
      if(ts.isVariableDeclaration(node) && node.name.getText(st)==='overlayActions') filter=node.initializer.arguments[0].getText(st);
      ts.forEachChild(node,scan);
    }
    scan(st);
    assert(filter, 'actual overlay filter not found');
    const visible=vm.runInNewContext(`(${filter})()`,{actions:input.response.actions,actionFilterIds:input.row.props.actionIds});
    assert(visible.some(a=>a.title==='Clear History'), 'selected-row Clear History visible');
  }
  console.log('actual renderer callback and overlay consumption passed');
})().catch(error=>{console.error(error);process.exitCode=1;});
