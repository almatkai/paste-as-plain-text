#!/usr/bin/env node
// Optional: node scripts/generate-oracle.cjs target/v1-check/package.json
// Expectations execute byte-exact source TS bodies with Raycast/OS/network mocks.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');
const {createRequire} = require('node:module');
const root = path.resolve(__dirname, '..');
const requireDeps = createRequire(path.resolve(process.argv[2] || 'target/v1-check/package.json'));
const ts = requireDeps('typescript');
const source = path.join(root,'tests/fixtures/source');
const defaults = {trimStart:true,trimEnd:true,cleanLineBreaks:false,autoFetchTitle:true,replaceClipboard:false,showTips:true};
function context(options={}) {
 const prefs={...defaults,...options.preferences}; const events=[]; const cache=new Map(); const modules=new Map();
 const clipboard={readText:async()=>options.text??'',read:async()=>({file:options.file}),paste:async(value)=>events.push(['paste',value]),copy:async(value)=>events.push(['copy',value])};
 const ray={Clipboard:clipboard,Cache:class{get(k){return cache.get(k)}set(k,v){cache.set(k,v)}},getPreferenceValues:()=>prefs,closeMainWindow:async()=>events.push(['hide']),Toast:{Style:{Animated:'animated',Failure:'failure'}},showToast:async(v)=>events.push(['toast',v]),showHUD:async(v)=>events.push(['hud',v]),updateCommandMetadata:async(v)=>events.push(['metadata',v]),captureException:()=>{}};
 function load(file) {
  if(modules.has(file))return modules.get(file).exports;
  const mod={exports:{}};modules.set(file,mod);
  const req=(name)=>{
   if(name==='@raycast/api')return ray;
   if(name==='axios')return {get:async()=>{if(options.titleFailure)throw Error('fixture failure');return {data:options.html??'<title>Fixture &amp; Title</title>'}}};
   if(name==='fs-extra')return {pathExistsSync:(s)=>(options.existing||[]).includes(s)};
   if(name.startsWith('.')) { const base=path.resolve(path.dirname(file),name);return load(['.ts','.tsx'].map(x=>base+x).find(fs.existsSync)); }
   return requireDeps(name);
  };
  const js=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText;
  vm.runInNewContext(`(function(require,module,exports){${js}\n})`,{console:{error(){},log(){}}},{filename:file})(req,mod,mod.exports);
  return mod.exports;
 }
 return {events,cache,utils:load(path.join(source,'src/utils/common-utils.ts')),run:async(mode)=>{const command=load(path.join(source,'src/paste-as-plain-text.tsx')).default;await command({arguments:mode===undefined?{}:{advancedPasteFormat:mode}})}};
}
(async()=>{
 const texts=['',' ','\t\r\n','\ufeff a \ufeff','\u0085a\u0085','\u200b x \u200b',' a \r\n \n b ',' a\u2028 b\u2029 c ','a\n \nb','\u00a0 hi\u3000'];
 const transforms=[];
 for(const text of texts)for(let bits=0;bits<8;bits++) {const preferences={trimStart:!!(bits&1),trimEnd:!!(bits&2),cleanLineBreaks:!!(bits&4)};transforms.push({text,preferences,expected:context({preferences}).utils.transform(text)});}
 const extracts=['https://a.test/x). and https://b.test','HTTPS://a.test','http:// https://yes.test','xhttps://a.test?q=1,','http://x\u0085tail','http://x\ufeffend','http://x\u2028end','http://','12abc','-12abc','x12.34abc','12,abc',',abc',',123,','abc-1,234.50x','１２３ and 42','1_2','99é','1e20','--12','1.2.3','-,foo','0','-0',',,,','no numbers','http://\u0085x'];
 const extraction=extracts.map(text=>({text,url:context().utils.extractUrl(text)??null,number:context().utils.extractNumber(text)}));
 const urls=['https://example.com','ftp://example.com/a','http://localhost','https://a.test/a(b)]','https://example.com?q=[x]','HTTPS://EXAMPLE.COM','http:example.com','https://127.0.0.1','http://[::1]:8080','http://[fe80::1%eth0]','http://foo_bar.com','http://example.com.','http://a.c','http://a.123','http://例子.中国','http://😀.com','http://user:pass@example.com','http://@example.com','http://a:b:c@example.com','http://example.com:','http://example.com:0','http://example.com:65536','http://example.com:001','file:///tmp/a','mailto:a@example.com','https://example.com/<x>','https://example.com/\u0085','http://a..com','http://-a.com','http://a-.com','http://１２３.com','http://example.com/a\nb','http://user:p@a@b.com','https://example.com/'+ 'x'.repeat(2100)];
 const isURL=requireDeps('validator').isURL;
 urls.push('http:127.0.0.1','http:user@example.com','http:0@example.com','http:user!@example.com','http://[::1]:','http://01.2.3.4','http://example.com:00000000000000000080','http://[::ffff:192.168.1.1]','http://[fe80::1%a.b]');
 const validation=urls.map(text=>({text,expected:isURL(text,{protocols:['http','https','ftp'],require_tld:true,require_protocol:true,require_host:true,require_port:false,require_valid_protocol:true})}));
 const jsonInputs=['{foo:\'bar\', trailing:[1,2,], //comment\n yes:true}','{z:1,"2":2,"1":1,a:3,"01":4}','{a:1,b:2,a:3}', '{"4294967295":1,z:0,"4294967294":2,"0":3,"01":4}','[Infinity,-Infinity,NaN,-NaN]', '+Infinity', '+NaN','[1e21,1e-7,1e-6,-0,1.0,9007199254740993]','0xdecaf','-0xFF','{bad:}','undefined','[true,null,"x"]','"\\uD83D\\uDE00"','"\\uD800"','9223372036854775808','1e400','0xffffffffffffffff','{line:"a\\\nb"}'];
 const JSON5=requireDeps('json5');
 const jsonCases=jsonInputs.map(text=>{try{return {text,expected:JSON.stringify(JSON5.parse(text),null,2)}}catch{return {text,expected:null}}});
 const modes=['','JSON','URL','Number','File','Markdown Link','Markdown Image','Unknown'];
 const cases=[];
 for(const mode of modes)for(const text of ['  hello \n world  ','  https://example.com/a  ','{a:1,}', 'value -1,234.50x', ' ', ''])cases.push({mode,text});
 cases.push({mode:'File',text:' /fixture/file name.txt ',existing:['/fixture/file name.txt']},{mode:'File',text:'file:///fixture/file%20name.txt',existing:['/fixture/file name.txt']},{mode:'File',text:'fallback',file:'/fixture/file name.txt',existing:['/fixture/file name.txt']},{mode:'File',text:'fallback',file:'/missing'},{mode:'Markdown Link',text:'https://example.com',titleFailure:true},{mode:'Markdown Image',text:'https://example.com',html:'<title>[x](y)</title>'});
 for(const pref of Object.keys(defaults))cases.push({mode:'Markdown Link',text:' \n https://example.com\n ',preferences:{[pref]:!defaults[pref]}});
 for(const c of cases){const x=context(c);await x.run(c.mode);c.pastes=x.events.filter(e=>e[0]==='paste').map(e=>e[1]);c.copies=x.events.filter(e=>e[0]==='copy').map(e=>e[1]);c.failure=x.events.some(e=>e[0]==='toast'&&e[1].style==='failure');}
 const provenance={upstreamCommit:'6734d85992290066fd31a7f5bee3cca3d3545a1a',ownCommit:'a3ae1c5abb3b9b79efd5bbbe0545f364b0251a3a',dependencies:Object.fromEntries(['json5','validator','cheerio','typescript'].map(n=>[n,requireDeps(n+'/package.json').version])),sourceHashes:{}};
 for(const f of ['src/paste-as-plain-text.tsx','src/utils/common-utils.ts','src/utils/paste-as-utils.ts','src/types/types.ts','package.json'])provenance.sourceHashes[f]=crypto.createHash('sha256').update(fs.readFileSync(path.join(source,f))).digest('hex');
 const argContext=context();const argumentsOracle=[undefined,'JSON',undefined,'',undefined,'URL',undefined].map(supplied=>({supplied:supplied??null,expected:argContext.utils.getArgument(supplied,'PasteAs')}));
 fs.writeFileSync(path.join(root,'tests/fixtures/oracle.json'),JSON.stringify({provenance,transforms,extraction,validation,jsonCases,cases,argumentsOracle},null,2)+'\n');
 console.log(`Generated ${transforms.length+extraction.length+validation.length+jsonCases.length+cases.length} source oracle cases`);
})();
