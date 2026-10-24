// Execute actual beta parser/reconciliation helpers, with imports stubbed at the UI boundary.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'..');
const ts=require(process.argv[2]||path.join(root,'target/v1-validation/node_modules/typescript'));
function load(filename,expose){
 const source=fs.readFileSync(path.join(root,'tests/fixtures/beta',filename),'utf8')+'\n'+expose.map(n=>'export { '+n+' };').join('\n');
 const code=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2020,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.React}}).outputText;
 const m={exports:{}};const react={forwardRef:fn=>fn,memo:fn=>fn};
 vm.runInNewContext(code,{module:m,exports:m.exports,require:()=>react,console},{filename});return m.exports;
}
const form=load('form-runtime.tsx',['collectFormItems']);const list=load('list-runtime.tsx',['parseListRows']);
const sample=JSON.parse(fs.readFileSync(path.join(root,'target/renderer-samples.json')));
const fields=view=>form.collectFormItems(view.root).filter(i=>i.type==='field').map(i=>i.field);
const initial=fields(sample.initial),height=fields(sample.height);
assert.equal(initial.length,3);assert.equal(initial[1].options.length,11);assert.equal(height[1].options.length,11);
const local={sizedropdown:'width',dropdown:'1440',textfield:'123456'};
const next=form.reconcileFormValues(height,local,initial);
assert.equal(next.textfield,'123456');assert.equal(next.sizedropdown,'height');assert.equal(next.dropdown,'1024');
assert.equal(fields(sample.invalid)[2].error,'Viewport and pixels must be integers');
assert.equal(fields(sample.cleared)[2].error,undefined);
const rows=list.parseListRows(sample.list.root);assert.equal(rows.length,1);assert.equal(rows[0].title,'1024px / 16px → 1.563vh');assert.equal(rows[0].actionIds.length,2);
assert.equal(sample.initial.actions[0].kind,'submit-form');
console.log('Actual beta parsers: all fields, 11 options per property, draft reconciliation, validation error, history List and action IDs passed');
