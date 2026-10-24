// Development-only oracle: transpiles and executes the pinned original TS/TSX.
// Run: node scripts/source-oracle.cjs [path/to/typescript]
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'..');
const ts=require(process.argv[2] || path.join(root,'target/v1-validation/node_modules/typescript'));
const source=path.join(root,'tests/fixtures/upstream');
const disk=new Map();let cached=[];let events=[];let hooks=[];let hook=0;
const date='2026-01-02T03:04:05.000Z';
const component=(name)=>Object.assign(()=>{}, {__name:name});
const Form=component('Form');for(const n of ['Dropdown','TextField','Description'])Form[n]=component('Form.'+n);Form.Dropdown.Item=component('Item');Form.Dropdown.Section=component('Section');
const Action=component('Action');Action.SubmitForm=component('Submit');
const api={Form,Action,ActionPanel:component('ActionPanel'),Toast:{Style:{Failure:'Failure'}},Clipboard:{copy:async(value)=>events.push(['copy',value])},closeMainWindow:()=>events.push(['hide']),showHUD:async(value)=>events.push(['hud',value]),showToast:async(value)=>events.push(['toast',value]),Cache:class {get(k){return disk.get(k)} set(k,v){disk.set(k,v)}}};
const react={createElement:(type,props,...children)=>({type,props:props||{},children}),useState:(initial)=>{const i=hook++;if(!(i in hooks))hooks[i]=initial;return[hooks[i],v=>{hooks[i]=v}]}};
const cache=new Map();
function load(name){
 if(cache.has(name))return cache.get(name).exports;
 const filename=path.join(source,name);const code=ts.transpileModule(fs.readFileSync(filename,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2020,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.React,esModuleInterop:true}}).outputText;
 const m={exports:{}};cache.set(name,m);
 const context={module:m,exports:m.exports,React:react,require:(id)=>id==='@raycast/api'?api:id==='react'?react:id==='@raycast/utils'?{useCachedState:()=>[cached,fn=>{cached=typeof fn==='function'?fn(cached):fn}]}:load(id.slice(2)+(id==='./types'?'.ts':'.ts')),Date:class extends Date{constructor(...args){super(...(args.length?args:[date]))}},console};
 vm.runInNewContext(code,context,{filename});return m.exports;
}
const utils=load('utils.ts'), history=load('history.ts');
const rows=[];
const viewports=['1440','1440.5','20px','1e2','1e-9999','1e9999','0x100','0x10z','0b10','0o10','  +16  ','-1','0','','bad','Infinity','1.2e+3','00001','1_000','1.','1e+21','9007199254740993','0x20000000000001','\ufeff16','\u008516','9999999999999999999999999999999999999999999999'];
const pixels=['16','16px','0','-1','','bad','0x10','1e3','1.9','9007199254740993','1'+'0'.repeat(21),'9'.repeat(400),'0x'+'f'.repeat(300)];
(async()=>{
 for(const command of ['width','height'])for(const viewport of viewports)for(const pixel of pixels){
  events=[];disk.clear();const cmd=load(command+'.tsx').default;await cmd({arguments:{[command]:viewport,pixels:pixel}});
  const raw=disk.get('history');const saved=raw?JSON.parse(raw):[];
  const copy=events.find(e=>e[0]==='copy');
  rows.push({command,viewport,pixels:pixel,accepted:saved.length>0,value:saved[0]?.data.value||null,label:saved[0]?.data.label||null,copy:!!copy});
 }
 // Render and invoke the original form's submit callback, including its second parseInt.
 const formCommand=load('index.tsx').default;
 const devices=[];
 for(const property of ['width','height']){
  hooks=[];hook=0;let tree=formCommand();
  if(property==='height'){tree.children[1].props.onChange('height');hook=0;tree=formCommand()}
  const dropdown=tree.children.find(n=>n&&n.type===Form.Dropdown&&n.props.id==='dropdown');
  const options=dropdown.children.flat().flatMap(group=>group.children.flat().map(n=>({group:group.props.title,...n.props})));
  assert.equal(options.length,11);devices.push({property,options});
  const submit=tree.props.actions.children[0].props.onSubmit;
  for(const {value:viewport} of options)for(const pixel of pixels){
   events=[];disk.clear();submit({dropdown:viewport,textfield:pixel});
   const saved=JSON.parse(disk.get('history')||'[]');const copy=events.find(e=>e[0]==='copy');
   rows.push({command:'index',property,viewport,pixels:pixel,accepted:saved.length>0,value:saved[0]?.data.value||null,label:saved[0]?.data.label||null,copy:!!copy,sourceCopy:copy?.[1]||null});
  }
 }
 // Execute original history code: newest first, duplicate retention, ten-item cap, removal no-op.
 disk.clear();for(let i=0;i<12;i++)utils.pixelToViewportUnit({pixels:'16',width:'100',height:null});
 const duplicates=JSON.parse(disk.get('history'));assert.equal(duplicates.length,10);assert(duplicates.every(e=>e.data.value==='16.000vw'));
 cached=[];const h=history.useHistory();h.add({data:{value:'1vw',label:'one'}});h.add({data:{value:'1vw',label:'one'}});assert.equal(cached.length,2);
 const item=cached[0];h.remove(item);assert.equal(cached.length,2);h.clear();assert.equal(cached.length,0);
 fs.writeFileSync(path.join(root,'tests/fixtures/source-oracle.json'),JSON.stringify({rows,devices,history:{duplicateCount:10,hookDuplicateCount:2,removeIsNoop:true}},null,2)+'\n');
 // Large binary64 vector: random bits, decimal rounding ties and neighbors, extremes.
 const bits=new Set(['0000000000000000','8000000000000000','0000000000000001','8000000000000001','7ff0000000000000','fff0000000000000','7ff8000000000000']);
 const buf=Buffer.alloc(8);function add(x){buf.writeDoubleBE(x);const b=buf.readBigUInt64BE();for(const d of [-1n,0n,1n]){const v=b+d;if(v>=0n&&v<=0xffffffffffffffffn)bits.add(v.toString(16).padStart(16,'0'))}}
 for(const x of [1e21,1e20,0.0005,-0.0005,1.2345,2.675,1000000000000000128])add(x);
 for(let i=-2500;i<=2500;i++)add((i+0.5)/1000);
 let random=0x123456789abcdefn;for(let i=0;i<20000;i++){random=BigInt.asUintN(64,random*6364136223846793005n+1442695040888963407n);bits.add(random.toString(16).padStart(16,'0'))}
 const fixed=[...bits].map(bits=>{buf.writeBigUInt64BE(BigInt('0x'+bits));const x=buf.readDoubleBE();return {bits,fixed:x.toFixed(3),string:String(x)}});
 fs.writeFileSync(path.join(root,'tests/fixtures/fixed-oracle.json'),JSON.stringify(fixed)+'\n');
 console.log(`Executed pinned sources: ${rows.length} conversions, 22 presets, history operations; ${fixed.length} binary64 formatting cases`);
})().catch(e=>{console.error(e);process.exitCode=1});
