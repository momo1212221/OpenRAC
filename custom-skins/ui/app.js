// SPDX-License-Identifier: MIT
import {api, isUnsupported} from "./bridge.js";
import {bytes, glbJson, pngSize, sha256, slug, textureNames, zip} from "./pack.js";

const STATIC_GAMES = [
  {id:"rac1", title:"Ratchet & Clank", versions:[{version:"pal",label:"PAL (Europe) v2.00"},{version:"ntsc",label:"NTSC-U"},{version:"pal1",label:"PAL (Europe) v1.00"},{version:"jp",label:"NTSC-J (Japan)"}]},
  {id:"rac2", title:"Ratchet & Clank: Going Commando", versions:[{version:"ntsc",label:"NTSC-U v1.01"},{version:"pal",label:"PAL (Europe)"},{version:"ntsc2",label:"NTSC-U v2.00"}]},
  {id:"rac3", title:"Ratchet & Clank: Up Your Arsenal", versions:[{version:"ntsc",label:"NTSC-U"},{version:"pal",label:"PAL (Europe)"}]},
  {id:"rac4", title:"Ratchet: Deadlocked", versions:[{version:"ntsc",label:"NTSC-U"}]},
  {id:"rac5", title:"Ratchet & Clank: Size Matters", versions:[{version:"ntsc",label:"NTSC-U"}]},
  {id:"rac6", title:"Ratchet & Clank: Secret Agent Clank", versions:[{version:"ntsc",label:"NTSC-U"}]},
];

const $ = (id) => document.getElementById(id);
const canvas = $("canvas");
const ctx = canvas.getContext("2d",{willReadFrequently:true});
const viewport = $("canvasViewport");
const state={info:null,targets:[],crossTarget:false,directApply:false,lookMode:"all",target:null,levelsFormat:null,groups:[],models:[],current:-1,history:[],redo:[],mode:"brush",down:false,last:null,zoom:1};

function say(text,bad=false){$("message").textContent=text;$("message").classList.toggle("error",bad)}
function targetKey(target){return target?target.game+"/"+target.version:""}
function sameCurrent(target){return !!target&&!!state.info&&target.game===state.info.game&&target.version===state.info.version}

function clearEditor(){
  state.groups=[];state.models=[];state.current=-1;state.history=[];state.redo=[];
  $("editor").hidden=true;$("textureList").replaceChildren();canvas.hidden=true;
  const empty=viewport.querySelector(".canvas-empty");if(empty)empty.hidden=false;updateProject()
}
function fallbackTargets(){return STATIC_GAMES.flatMap((game)=>game.versions.map((version)=>({game:game.id,gameTitle:game.title,version:version.version,label:version.label,key:game.id+"/"+version.version})))}
async function discoverTargets(){
  try{const found=await api("targets");if(!Array.isArray(found)||!found.length)throw new Error("no game targets returned");state.crossTarget=true;state.targets=found}
  catch(error){state.crossTarget=false;state.targets=fallbackTargets();if(!isUnsupported(error,"targets"))console.warn(error)}
}
function uniqueGames(){const seen=new Map();for(const target of state.targets)if(!seen.has(target.game))seen.set(target.game,target.gameTitle||target.game.toUpperCase());for(const fallback of STATIC_GAMES)if(!seen.has(fallback.id))seen.set(fallback.id,fallback.title);return[...seen.entries()].map(([id,title])=>({id,title}))}
function versionsFor(game){const found=state.targets.filter((target)=>target.game===game);if(found.length)return found;const fallback=STATIC_GAMES.find((item)=>item.id===game);return(fallback?.versions||[]).map((version)=>({game,gameTitle:fallback.title,version:version.version,label:version.label,key:game+"/"+version.version}))}
function fillGames(){const select=$("game");select.replaceChildren();for(const game of uniqueGames()){const option=document.createElement("option");option.value=game.id;option.textContent=game.title;select.append(option)}select.value=state.info?.game||uniqueGames()[0]?.id||"rac1";fillVersions(state.info?.version||null)}
function fillVersions(preferred=null){const game=$("game").value,select=$("version");select.replaceChildren();const versions=versionsFor(game);for(const target of versions){const option=document.createElement("option");option.value=target.version;option.textContent=target.label||target.region||target.version;select.append(option)}select.value=versions.some((v)=>v.version===preferred)?preferred:(versions[0]?.version||"");chooseTarget()}
function chooseTarget(){const game=$("game").value,version=$("version").value;state.target=state.targets.find((target)=>target.game===game&&target.version===version)||versionsFor(game).find((target)=>target.version===version)||null;clearEditor();updateTargetHint()}
function updateTargetHint(){
  const target=state.target;if(!target){$("targetHint").textContent="No target selected.";$("scan").disabled=true;return}
  if(!state.crossTarget&&!sameCurrent(target)){$("targetHint").textContent="This launcher exposes only the game that opened the mod. The Skin Studio host extension enables cross-game asset access.";$("scan").disabled=true}
  else{$("targetHint").textContent=state.crossTarget?"Cross-game OpenRAC asset access is available.":"Using the current game's OpenRAC assets.";$("scan").disabled=false}
}
async function listLevels(){if(state.crossTarget)return api("targetList",{game:state.target.game,version:state.target.version,root:"levels"});if(!sameCurrent(state.target))throw new Error("This launcher cannot read another game's prepared levels.");return api("list",{root:"levels"})}
async function readLevel(path){const value=state.crossTarget?await api("targetRead",{game:state.target.game,version:state.target.version,root:"levels",path}):await api("read",{root:"levels",path});return bytes(value)}
async function readJson(path){return JSON.parse(new TextDecoder().decode(await readLevel(path)))}
function levelDir(path){const found=path.match(/^(level_\d{2})\/manifest\.json$/);return found?found[1]:null}
function heroClasses(classes){const ids=Object.keys(classes||{}).map(Number).filter(Number.isInteger);return ids.filter((id)=>id===0||(state.lookMode==="all"&&id>=6000&&id<6020)).sort((a,b)=>a-b)}

async function scan(){
  if(!state.target)return;$("scan").disabled=true;$("apply").disabled=true;$("export").disabled=true;say("Reading prepared OpenRAC hero assets…");clearEditor();
  try{
    const listed=await listLevels();const manifests=listed.map((f)=>f.path).filter((path)=>/^level_\d{2}\/manifest\.json$/.test(path));
    if(!manifests.length)throw new Error("No prepared editor levels exist for "+targetKey(state.target)+". OpenRAC cannot build a skin for this target yet.");
    const byHash=new Map(),models=[];let levelsFormat=null;
    for(const manifestPath of manifests){
      const dir=levelDir(manifestPath),manifest=await readJson(manifestPath);levelsFormat??=Number.isInteger(manifest.format)?manifest.format:null;const classes=manifest?.classes?.moby||{};
      for(const classId of heroClasses(classes)){
        const hero=classes[String(classId)]??classes[classId];if(!hero||typeof hero.mesh!=="string"||!hero.mesh)continue;
        const modelPath=dir+"/"+hero.mesh;let modelBytes;try{modelBytes=await readLevel(modelPath)}catch{continue}
        const names=textureNames(glbJson(modelBytes));if(!names.length)continue;
        models.push({level:dir,path:modelPath,classId,name:hero.name||(classId===0?"Ratchet":"Ratchet look "+(classId-6000))});
        for(let slot=0;slot<names.length;slot++){
          const name=names[slot],path=dir+"/textures/"+name+".png";let original;try{original=await readLevel(path)}catch{continue}
          const hash=await sha256(original),[width,height]=pngSize(original);let group=byHash.get(hash);
          if(!group){group={hash,original,width,height,edited:null,dirty:false,places:[]};byHash.set(hash,group)}
          if(!group.places.some((place)=>place.path===path&&place.classId===classId&&place.slot===slot))group.places.push({level:dir,path,name,classId,slot,count:names.length})
        }
      }
    }
    const groups=[...byHash.values()];if(!groups.length)throw new Error("OpenRAC found no readable Ratchet class-0/look textures in the prepared levels for "+targetKey(state.target)+".");
    state.levelsFormat=levelsFormat||4;state.groups=groups;state.models=models;$("editor").hidden=false;
    $("sourceState").textContent="Loaded "+groups.length+" unique Ratchet texture"+(groups.length===1?"":"s")+".";
    await showGroup(0,false);say("Skin loaded from "+targetKey(state.target)+". Edit a texture, then apply it to the game or export the pack.")
  }catch(error){say(error instanceof Error?error.message:String(error),true);$("sourceState").textContent="No usable skin assets loaded."}
  finally{$("scan").disabled=false;updateProject();updateTargetHint()}
}

function currentGroup(){return state.current>=0?state.groups[state.current]:null}
function changedGroups(){return state.groups.filter((group)=>group.edited||group.dirty)}
function updateProject(){
  const changed=changedGroups().length;$("modifiedCount").textContent=changed+" modified";$("textureCount").textContent=String(state.groups.length);$("modelCount").textContent=String(state.models.length);$("apply").disabled=changed===0;$("export").disabled=changed===0;
  const group=currentGroup();if(group){const classes=[...new Set(group.places.map((p)=>p.classId))].sort((a,b)=>a-b);$("texInfo").textContent="Texture "+(state.current+1)+"/"+state.groups.length+" · "+group.width+"×"+group.height+" · classes "+classes.join(", ")+" · "+group.places.length+" use"+(group.places.length===1?"":"s")}else $("texInfo").textContent="No texture selected"
}
async function imageFrom(data){const url=URL.createObjectURL(new Blob([data],{type:"image/png"}));try{const image=new Image();await new Promise((resolve,reject)=>{image.onload=resolve;image.onerror=()=>reject(new Error("Could not decode the PNG."));image.src=url});return image}finally{setTimeout(()=>URL.revokeObjectURL(url),0)}}
async function canvasPng(){const blob=await new Promise((resolve)=>canvas.toBlob(resolve,"image/png"));if(!blob)throw new Error("Could not encode the edited PNG.");return new Uint8Array(await blob.arrayBuffer())}
async function saveCurrent(){const group=currentGroup();if(!group||!group.dirty)return;const data=await canvasPng();group.edited=(await sha256(data))===group.hash?null:data;group.dirty=false}
function markDirty(){const group=currentGroup();if(!group)return;group.dirty=true;updateProject()}
function setEditing(enabled){for(const id of["undo","redo","reset","zoomIn","zoomOut","fit","applyTint","gray"])$(id).disabled=!enabled}
function snapshot(){if(!canvas.width||!canvas.height)return;state.history.push(ctx.getImageData(0,0,canvas.width,canvas.height));if(state.history.length>40)state.history.shift();state.redo=[];$("undo").disabled=state.history.length===0;$("redo").disabled=true}
function setZoom(value){state.zoom=Math.max(.15,Math.min(8,value));canvas.style.width=Math.max(1,Math.round(canvas.width*state.zoom))+"px";canvas.style.height=Math.max(1,Math.round(canvas.height*state.zoom))+"px";$("zoomLabel").textContent=Math.round(state.zoom*100)+"%"}
function fitCanvas(){if(state.current<0||!canvas.width||!canvas.height)return;const width=Math.max(120,viewport.clientWidth-60),height=Math.max(120,viewport.clientHeight-60);setZoom(Math.min(1.5,width/canvas.width,height/canvas.height))}
async function showGroup(index,save=true){if(save&&state.current>=0)await saveCurrent();const group=state.groups[index];if(!group)return;state.current=index;state.history=[];state.redo=[];const source=group.edited||group.original,image=await imageFrom(source);canvas.width=image.naturalWidth;canvas.height=image.naturalHeight;ctx.clearRect(0,0,canvas.width,canvas.height);ctx.drawImage(image,0,0);canvas.hidden=false;const empty=viewport.querySelector(".canvas-empty");if(empty)empty.hidden=true;setEditing(true);requestAnimationFrame(fitCanvas);renderTextureList();updateProject()}
function renderTextureList(){const list=$("textureList");list.replaceChildren();state.groups.forEach((group,index)=>{const button=document.createElement("button");button.type="button";button.className="texture-item";if(index===state.current)button.classList.add("active");if(group.edited||group.dirty)button.classList.add("modified");const image=document.createElement("img");image.className="thumb";image.alt="";const url=URL.createObjectURL(new Blob([group.edited||group.original],{type:"image/png"}));image.src=url;image.onload=image.onerror=()=>URL.revokeObjectURL(url);const meta=document.createElement("div");meta.className="texture-meta";const title=document.createElement("strong");title.textContent="Skin texture "+(index+1);const detail=document.createElement("span");const classes=[...new Set(group.places.map((p)=>p.classId))].sort((a,b)=>a-b);detail.textContent=group.width+"×"+group.height+" · "+(classes.length>1?classes.length+" looks":"class "+classes[0]);meta.append(title,detail);button.append(image,meta);button.onclick=()=>void showGroup(index).catch((error)=>say(String(error),true));list.append(button)})}
function point(event){const rect=canvas.getBoundingClientRect();return{x:(event.clientX-rect.left)*canvas.width/rect.width,y:(event.clientY-rect.top)*canvas.height/rect.height}}
function stroke(a,b){ctx.save();ctx.globalAlpha=Number($("opacity").value)/100;ctx.globalCompositeOperation=state.mode==="erase"?"destination-out":"source-over";ctx.strokeStyle=$("color").value;ctx.lineWidth=Number($("size").value);ctx.lineCap="round";ctx.lineJoin="round";ctx.beginPath();ctx.moveTo(a.x,a.y);ctx.lineTo(b.x,b.y);ctx.stroke();ctx.restore()}
canvas.addEventListener("pointerdown",(event)=>{if(state.current<0)return;snapshot();state.down=true;state.last=point(event);canvas.setPointerCapture(event.pointerId);stroke(state.last,{x:state.last.x+.01,y:state.last.y+.01});markDirty()});
canvas.addEventListener("pointermove",(event)=>{if(!state.down||!state.last)return;const next=point(event);stroke(state.last,next);state.last=next});
for(const event of["pointerup","pointercancel"])canvas.addEventListener(event,()=>{state.down=false;state.last=null});
$("undo").onclick=()=>{const previous=state.history.pop();if(!previous)return;state.redo.push(ctx.getImageData(0,0,canvas.width,canvas.height));ctx.putImageData(previous,0,0);$("undo").disabled=state.history.length===0;$("redo").disabled=state.redo.length===0;markDirty()};
$("redo").onclick=()=>{const next=state.redo.pop();if(!next)return;state.history.push(ctx.getImageData(0,0,canvas.width,canvas.height));ctx.putImageData(next,0,0);$("undo").disabled=state.history.length===0;$("redo").disabled=state.redo.length===0;markDirty()};
$("reset").onclick=async()=>{const group=currentGroup();if(!group)return;snapshot();const image=await imageFrom(group.original);canvas.width=image.naturalWidth;canvas.height=image.naturalHeight;ctx.clearRect(0,0,canvas.width,canvas.height);ctx.drawImage(image,0,0);group.edited=null;group.dirty=false;fitCanvas();renderTextureList();updateProject();say("Restored the original texture.")};
$("brush").onclick=()=>{state.mode="brush";$("brush").classList.add("selected");$("erase").classList.remove("selected")};$("erase").onclick=()=>{state.mode="erase";$("erase").classList.add("selected");$("brush").classList.remove("selected")};
$("size").oninput=()=>$("sizeOut").textContent=$("size").value+" px";$("opacity").oninput=()=>$("opacityOut").textContent=$("opacity").value+"%";$("tint").oninput=()=>$("tintOut").textContent=$("tint").value+"%";
$("applyTint").onclick=()=>{if(state.current<0)return;snapshot();const image=ctx.getImageData(0,0,canvas.width,canvas.height),mix=Number($("tint").value)/100,hex=$("color").value,tr=parseInt(hex.slice(1,3),16),tg=parseInt(hex.slice(3,5),16),tb=parseInt(hex.slice(5,7),16);for(let i=0;i<image.data.length;i+=4){image.data[i]=Math.round(image.data[i]*(1-mix)+tr*mix);image.data[i+1]=Math.round(image.data[i+1]*(1-mix)+tg*mix);image.data[i+2]=Math.round(image.data[i+2]*(1-mix)+tb*mix)}ctx.putImageData(image,0,0);markDirty()};
$("gray").onclick=()=>{if(state.current<0)return;snapshot();const image=ctx.getImageData(0,0,canvas.width,canvas.height);for(let i=0;i<image.data.length;i+=4){const y=Math.round(image.data[i]*.2126+image.data[i+1]*.7152+image.data[i+2]*.0722);image.data[i]=image.data[i+1]=image.data[i+2]=y}ctx.putImageData(image,0,0);markDirty()};
$("replace").onchange=async()=>{const file=$("replace").files?.[0],group=currentGroup();if(!file||!group)return;try{const data=new Uint8Array(await file.arrayBuffer()),image=await imageFrom(data);if(Math.abs(image.naturalWidth/image.naturalHeight-group.width/group.height)>1e-6)throw new Error("Keep the original aspect ratio "+group.width+":"+group.height+".");snapshot();canvas.width=image.naturalWidth;canvas.height=image.naturalHeight;ctx.clearRect(0,0,canvas.width,canvas.height);ctx.drawImage(image,0,0);markDirty();fitCanvas();say("Loaded "+file.name+".")}catch(error){say(error instanceof Error?error.message:String(error),true)}finally{$("replace").value=""}};
$("zoomIn").onclick=()=>setZoom(state.zoom*1.25);$("zoomOut").onclick=()=>setZoom(state.zoom/1.25);$("fit").onclick=fitCanvas;

function addReplace(replaces,level,name){const entry=replaces[level]||=( {} );const list=entry.textures||=( [] );if(!list.includes(name))list.push(name)}
async function makePack(){
  await saveCurrent();renderTextureList();updateProject();const changed=state.groups.filter((group)=>group.edited);if(!changed.length)throw new Error("Nothing changed yet. Edit or replace at least one Ratchet texture.");
  const packName=$("packName").value.trim()||"My Ratchet skin",author=$("author").value.trim();
  const manifest={format:1,game:state.target.game,version:state.target.version,name:packName,author,description:"Ratchet skin made with Custom Skin Studio 0.2.0.",levels_format:state.levelsFormat,replaces:{},originals:{}};
  const fileMap=new Map();
  for(const group of changed)for(const place of group.places){fileMap.set(place.path,group.edited);addReplace(manifest.replaces,place.level,place.name);const original=manifest.originals[place.path]||=( {sha256:group.hash,size:[group.width,group.height],used_by:[]} );const use=["moby",place.classId,place.slot,place.count];if(!original.used_by.some((value)=>JSON.stringify(value)===JSON.stringify(use)))original.used_by.push(use)}
  for(const value of Object.values(manifest.replaces))value.textures.sort();
  const files=[{name:"openrac-mod.json",data:new TextEncoder().encode(JSON.stringify(manifest,null,2)+"\n")},...[...fileMap.entries()].sort(([a],[b])=>a.localeCompare(b)).map(([name,data])=>({name,data}))];
  return{payload:zip(files),name:slug(packName),manifest,changed:changed.length}
}
async function exportPack(){$("export").disabled=true;try{const pack=await makePack(),buffer=pack.payload.buffer.slice(pack.payload.byteOffset,pack.payload.byteOffset+pack.payload.byteLength);const saved=await api("save",{name:pack.name,extension:"zip",bytes:buffer},[buffer]);say(saved?"Saved "+saved+". Add it through Mods → Add a mod; it replaces Ratchet's edited textures in game.":"Save cancelled.")}catch(error){say(error instanceof Error?error.message:String(error),true)}finally{updateProject()}}
async function applyPack(){$("apply").disabled=true;try{const pack=await makePack(),transfer=pack.payload.buffer.slice(pack.payload.byteOffset,pack.payload.byteOffset+pack.payload.byteLength);const result=await api("applyPack",{game:state.target.game,version:state.target.version,name:pack.name,bytes:transfer},[transfer],300000);state.directApply=true;say(result?.text||("Applied "+pack.changed+" edited texture groups to "+targetKey(state.target)+"."));$("applyHint").textContent="Installed as a normal OpenRAC overrides pack. It is active for the selected game.";$("capabilities").textContent=capabilityText()}catch(error){if(isUnsupported(error,"applyPack")){state.directApply=false;say("This launcher does not have direct Skin Studio pack installation yet. Use Export ZIP and add it through Mods → Add a mod.",true)}else say(error instanceof Error?error.message:String(error),true)}finally{updateProject()}}
function capabilityText(){return["Cross-game asset access: "+(state.crossTarget?"available":"current game only"),"Direct pack installation: "+(state.directApply?"confirmed":"not yet confirmed"),"Pack format: OpenRAC overrides format 1"].join(" · ")}

$("game").onchange=()=>fillVersions();$("version").onchange=chooseTarget;$("scan").onclick=()=>void scan();$("export").onclick=()=>void exportPack();$("apply").onclick=()=>void applyPack();
for(const button of $("lookMode").querySelectorAll("button"))button.onclick=()=>{state.lookMode=button.dataset.value;for(const item of $("lookMode").querySelectorAll("button"))item.classList.toggle("selected",item===button);clearEditor();$("sourceState").textContent=state.lookMode==="all"?"All Ratchet looks will be scanned (base class plus exported armour/look variants).":"Only Ratchet's base class will be scanned."};
window.addEventListener("keydown",(event)=>{const modifier=event.metaKey||event.ctrlKey;if(modifier&&event.key.toLowerCase()==="z"){event.preventDefault();(event.shiftKey?$("redo"):$("undo")).click()}else if(modifier&&(event.key==="+"||event.key==="=")){event.preventDefault();$("zoomIn").click()}else if(modifier&&event.key==="-"){event.preventDefault();$("zoomOut").click()}else if(modifier&&event.key==="0"){event.preventDefault();$("fit").click()}});

async function start(){setEditing(false);state.info=await api("info");await discoverTargets();fillGames();$("subtitle").textContent="Mod 0.2.0 · OpenRAC replacement assets · "+state.info.game+"/"+state.info.version;$("capabilities").textContent=capabilityText();updateTargetHint();updateProject();say("Ready. Choose a game and load its Ratchet skin.")}
start().catch((error)=>say(error instanceof Error?error.message:String(error),true));
