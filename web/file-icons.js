// Local artwork only. File contents and SVN state are never consulted.
export const FILE_ICONS=Object.freeze(['php','javascript','typescript','vue','react','react-ts','html','css','sass','less','python','java','go','rust','c','cpp','shell','json','yaml','xml','toml','ini','env','sql','graphql','markdown','text','log','image','svg','pdf','archive','font','composer','npm','docker','git','lock','test','file']);
export const FOLDER_ICONS=Object.freeze(['folder','folder-open','folder-src','folder-api','folder-controller','folder-model','folder-view','folder-components','folder-config','folder-assets','folder-tests','folder-database','folder-deps','folder-ide']);
const names=new Map([
  ['composer.json','composer'],['package.json','npm'],['.npmrc','npm'],['.npmignore','npm'],
  ['dockerfile','docker'],['containerfile','docker'],['.dockerignore','docker'],['docker-compose.yml','docker'],['docker-compose.yaml','docker'],['compose.yml','docker'],['compose.yaml','docker'],
  ['.gitignore','git'],['.gitattributes','git'],['.gitmodules','git'],['.gitkeep','git'],['.gitmessage','git'],
  ['package-lock.json','lock'],['npm-shrinkwrap.json','lock'],['pnpm-lock.yaml','lock'],['yarn.lock','lock'],['bun.lockb','lock'],['bun.lock','lock'],['composer.lock','lock'],['cargo.lock','lock'],['poetry.lock','lock'],['pipfile.lock','lock'],['gemfile.lock','lock'],
  ['.editorconfig','ini'],['.eslintrc','json'],['.prettierrc','json'],['.babelrc','json'],['.bowerrc','json'],
  ['.bashrc','shell'],['.bash_profile','shell'],['.zshrc','shell'],['.zprofile','shell'],['.profile','shell'],
]);
const suffixes=new Map();
for(const [id,extensions] of [
  ['php','php phtml php3 php4 php5 php7 php8 phps'],['javascript','js mjs cjs'],['typescript','ts mts cts'],['vue','vue'],['react','jsx'],['react-ts','tsx'],['html','html htm xhtml'],['css','css'],['sass','sass scss'],['less','less'],['python','py pyi pyw'],['java','java'],['go','go'],['rust','rs'],['c','c h'],['cpp','cpp cc cxx c++ hpp hh hxx'],['shell','sh bash zsh fish'],
  ['json','json jsonc json5'],['yaml','yaml yml'],['xml','xml xsd xsl xslt plist iml'],['toml','toml'],['ini','ini conf cfg properties'],['env','env'],['sql','sql'],['graphql','graphql gql'],['markdown','md markdown mdx'],['text','txt text'],['log','log'],['image','png jpg jpeg gif webp avif bmp ico icns tif tiff'],['svg','svg'],['pdf','pdf'],['archive','zip tar gz tgz bz2 xz zst rar 7z'],['font','ttf otf woff woff2'],['lock','lock'],
])for(const extension of extensions.split(' '))suffixes.set(extension,id);
const folders=new Map();
for(const [id,aliases] of [
  ['folder-src','src source sources application app'],['folder-api','api apis'],['folder-controller','controller controllers'],['folder-model','model models entity entities'],['folder-view','view views template templates'],['folder-components','component components'],['folder-config','config configs configuration settings'],['folder-assets','asset assets public static resources'],['folder-tests','test tests __tests__ spec specs'],['folder-database','database databases db sql migrations'],['folder-deps','vendor node_modules dependencies packages'],['folder-ide','.openidea .idea .vscode'],
])for(const alias of aliases.split(' '))folders.set(alias,id);
const cache=new Map();
const CACHE_LIMIT=2048;
export function iconFor(path,{directory=false,expanded=false}={}) {
  // Protocol paths are '/'-separated; tolerate Windows paths for display only.
  const basename=String(path).replace(/\\/g,'/').replace(/\/+$/,'').split('/').at(-1);
  const name=basename.toLowerCase();
  const phpTest=/(?:Test|Tests)\.[pP][hH][pP]$/.test(basename)||/(?:^|[._-])tests?\.php$/.test(name);
  const key=`${directory?'d':'f'}:${directory&&expanded?'o':'c'}:${phpTest?'t:':''}${name}`;
  if(cache.has(key))return cache.get(key);
  let icon;
  if(directory)icon=folders.get(name)||(expanded?'folder-open':'folder');
  else if(names.has(name))icon=names.get(name);
  else if(name==='.env'||name.startsWith('.env.'))icon='env';
  else if(/(?:^|\.)(?:test|spec)\.(?:[cm]?[jt]sx?|vue|py|go|rs|php)$/.test(name)||phpTest||name.endsWith('_test.go')||/^test_.+\.py$/.test(name))icon='test';
  else {
    const parts=name.split('.');
    for(let i=1;i<parts.length;i++){icon=suffixes.get(parts.slice(i).join('.'));if(icon)break;}
    icon||='file';
  }
  if(cache.size>=CACHE_LIMIT)cache.clear();
  cache.set(key,icon);return icon;
}
