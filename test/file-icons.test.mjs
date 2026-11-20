import test from 'node:test';
import assert from 'node:assert/strict';
import {iconFor, FILE_ICONS, FOLDER_ICONS} from '../web/file-icons.js';

test('approved icon inventory covers 40 file and 14 folder kinds',()=>{
  assert.equal(FILE_ICONS.length,40);assert.equal(FOLDER_ICONS.length,14);
  assert.equal(new Set([...FILE_ICONS,...FOLDER_ICONS]).size,54);
  const files={php:'Page.PHP',javascript:'main.cjs',typescript:'a.d.ts',vue:'Page.vue',react:'App.jsx','react-ts':'App.tsx',html:'index.htm',css:'main.css',sass:'main.scss',less:'main.less',python:'app.py',java:'App.java',go:'main.go',rust:'lib.rs',c:'main.c',cpp:'main.hpp',shell:'build.zsh',json:'a.jsonc',yaml:'a.yml',xml:'a.xsd',toml:'plugin.toml',ini:'a.conf',env:'.env.production',sql:'schema.sql',graphql:'query.gql',markdown:'README.md',text:'a.txt',log:'server.log',image:'photo.webp',svg:'icon.svg',pdf:'a.pdf',archive:'a.tar.gz',font:'a.woff2',composer:'composer.json',npm:'package.json',docker:'Dockerfile',git:'.gitignore',lock:'pnpm-lock.yaml',test:'UserTest.php',file:'unknown.bin'};
  for(const [expected,path] of Object.entries(files))assert.equal(iconFor('src/'+path),expected,path);
  const folders={'folder':'misc','folder-open':'misc','folder-src':'application','folder-api':'api','folder-controller':'controllers','folder-model':'models','folder-view':'templates','folder-components':'components','folder-config':'config','folder-assets':'public','folder-tests':'tests','folder-database':'sql','folder-deps':'vendor','folder-ide':'.openidea'};
  for(const [expected,path] of Object.entries(folders))assert.equal(iconFor(path,{directory:true,expanded:expected==='folder-open'}),expected,path);
});
test('precise names and compound extensions take precedence without changing targets',()=>{
  const cases=[['COMPOSER.JSON','composer'],['docker-compose.yml','docker'],['compose.yaml','docker'],['package-lock.json','lock'],['composer.lock','lock'],['Cargo.lock','lock'],['test/Fundorder.php','php'],['src/FundorderTest.php','test'],['Test.php','test'],['testament.php','php'],['contest.php','php'],['latest.php','php'],['user_test.php','test'],['main.test.tsx','test'],['Page.spec.vue','test'],['src/foo.test.json','json'],['dockerfile.php','php'],['D:\\project\\src\\App.tsx','react-ts'],['.env.example','env'],['config.env','env'],['prefix.env.production','file'],['.eslintrc','json'],['.editorconfig','ini'],['tsconfig.json','json'],['.gitkeep','git']];
  for(const [path,id] of cases)assert.equal(iconFor(path),id,path);
  assert.equal(iconFor('some.php',{directory:true}),'folder');
  assert.equal(iconFor('src/CONFIG',{directory:true,expanded:true}),'folder-config');
  assert.equal(iconFor('constructor'),'file');assert.equal(iconFor('__proto__'),'file');
  assert.equal(iconFor('src/<img src=x onerror=alert(1)>.unknown'),'file');
  for(let i=0;i<10000;i++)assert.equal(iconFor(`other/${i}.php`),'php');
  assert.equal(iconFor('composer.json'),'composer');assert.equal(iconFor('README.MD'),'markdown');
});
