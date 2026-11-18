/* Shared command-list/form/output controller. Execution is owned by the Rust plugin. */
(() => {
  const drafts=new Map();
  const tr=(key,fallback,args)=>window.uiKitI18n?.t(key,fallback,args)??fallback;
  window.uiKitCommandWorkspace={mount(root,bridge){
    const $=id=>root.querySelector(`#${id}`),element=(tag,text)=>{const node=document.createElement(tag);if(text!=null)node.textContent=text;return node;};
    const scope=bridge.project||"";
    function draftFor(runner){const key=`${scope}\n${runner}`;if(!drafts.has(key)){if(drafts.size>=16)drafts.delete(drafts.keys().next().value);drafts.set(key,{selected:"",values:{},recent:[]});}return drafts.get(key);}
    let draft=draftFor("php");
    let runner="php",targetLabel="",commands=[],token=null,selected=draft.selected,job=null,history=[],busy=false,disposed=false,timer=null,revision=0,valid=false,previewPending=false,pending="",pendingCommand="",supportsExtra=false;
    const fields=new Map();
    const running=()=>Boolean(job?.running);
    function controls(){if($("command-additional"))$("command-additional").hidden=!supportsExtra;$("command-docker-target").hidden=runner!=="docker";$("command-docker-target").disabled=busy||running();$("command-runner-target").hidden=runner!=="docker";$("command-runner-target").textContent=targetLabel||tr("command.dockerTarget","Choose a Docker target");$("command-runner-target").title=targetLabel;$("command-runner").value=runner;$("command-runner").disabled=busy||running();const chosen=commands.some(c=>c.name===selected);$("discover").disabled=busy||running();$("load-commands").disabled=busy||running();$("command-editor").hidden=!chosen&&!job;$("command-empty").hidden=chosen||Boolean(job);$("run").disabled=busy||running()||!valid;$("terminal").disabled=busy||running()||!chosen;$("cancel").hidden=!running();$("cancel").disabled=busy||!running();$("copy").hidden=!job||!(job.stdout||job.stderr);$("copy").disabled=busy||!job;$("rerun").hidden=!job||running();$("rerun").disabled=busy||running()||!job||(job.runner||"php")!==runner||(runner==="docker"&&(job.targetLabel||"")!==targetLabel);}
    function send(request){if(disposed||busy)return false;clearTimeout(timer);busy=true;pending=request.op;pendingCommand=request.command||"";controls();bridge.postMessage(JSON.stringify(request));return true;}
    function values(){const result={};for(const [key,field] of fields){if(field.bare?.checked)result[key]=true;else if(field.input.type==="checkbox")result[key]=field.input.checked;else if(field.repeated)result[key]=field.input.value.split("\n").filter(v=>v!=="");else if(field.input.value!=="")result[key]=field.input.value;}return result;}
    function extraPayload(record){return supportsExtra?{additionalOptions:record?.additionalOptions??$("command-extra-options")?.value??"",additionalArguments:record?.additionalArguments??$("command-extra-arguments")?.value??""}:{};}
    function saveExtra(){if(!supportsExtra||!selected)return;draft.additionalOptions||={};draft.additionalArguments||={};draft.additionalOptions[selected]=$("command-extra-options")?.value||"";draft.additionalArguments[selected]=$("command-extra-arguments")?.value||"";}
    function change(){draft.values[selected]=values();saveExtra();revision++;valid=false;previewPending=true;controls();clearTimeout(timer);timer=setTimeout(preview,180);}
    function preview(){if(!selected||!token)return;previewPending=true;if(send({op:"preview",command:selected,token,values:values(),revision,...extraPayload()}))previewPending=false;}
    function remember(names){const recent=[...new Set([...names,...(draft.recent||[])])].filter(name=>commands.some(c=>c.name===name)).slice(0,8);if(JSON.stringify(recent)!==JSON.stringify(draft.recent||[])){draft.recent=recent;list();}}
    function list(){
      const container=$("commands"),scroll=container.scrollTop,focused=root.activeElement?.closest?.("#commands button"),focusName=focused?.dataset.command,focusGroup=focused?.closest("[data-command-group]")?.dataset.commandGroup;
      const query=$("command-search").value.trim().toLowerCase(),visible=commands.filter(c=>`${c.name} ${c.description}`.toLowerCase().includes(query));container.replaceChildren();
      const groups=new Map();
      for(const command of visible){const prefix=command.name.includes(":")?command.name.split(":")[0]:"";if(!groups.has(prefix))groups.set(prefix,[]);groups.get(prefix).push(command);}
      function group(key,label,entries){
        if(!entries.length)return;
        const section=element("section");section.className="command-group";section.dataset.commandGroup=key;
        const heading=element("h3",label);heading.className="ui-group-header";section.append(heading);section.setAttribute("aria-label",label);
        for(const command of entries){const button=element("button");button.type="button";button.className="ui-button command-item";button.dataset.command=command.name;button.setAttribute("aria-label",command.name);button.title=command.description;button.setAttribute("aria-pressed",String(selected===command.name));
          const name=element("span",command.name);name.className="command-item-name";button.append(name);
          if(command.description){const description=element("small",command.description);description.className="command-item-description";button.append(description);}
          button.onclick=()=>choose(command.name);section.append(button);
        }container.append(section);
      }
      group("recent",tr("command.recent","Recent"),(draft.recent||[]).map(name=>visible.find(c=>c.name===name)).filter(Boolean));
      for(const key of [...groups.keys()].sort((a,b)=>a===""?-1:b===""?1:a.localeCompare(b))){group(key?`namespace:${key}`:"general",key?key[0].toUpperCase()+key.slice(1):tr("command.general","General"),groups.get(key).sort((a,b)=>a.name.localeCompare(b.name)));}
      if(commands.length&&!visible.length)container.append(element("p",tr("command.noMatching","No matching commands.")));
      container.scrollTop=scroll;
      if(focusName){const section=[...container.children].find(node=>node.dataset.commandGroup===focusGroup);[...(section||container).querySelectorAll("button")].find(button=>button.dataset.command===focusName)?.focus({preventScroll:true});}
    }
    function choose(name,restored,additional){if(selected&&fields.size)draft.values[selected]=values();saveExtra();selected=name;draft.selected=name;const command=commands.find(c=>c.name===name);fields.clear();$("arguments").replaceChildren();$("command-name").textContent=name||tr("command.choose","Choose a command");$("command-description").textContent=command?.description||"";valid=false;list();if(!command){controls();return;}
      const saved=restored||draft.values[name]||{};
      if($("command-extra-options"))$("command-extra-options").value=additional?.additionalOptions??draft.additionalOptions?.[name]??"";
      if($("command-extra-arguments"))$("command-extra-arguments").value=additional?.additionalArguments??draft.additionalArguments?.[name]??"";
      let index=0;const sections=new Map(),globals=new Set(["help","quiet","verbose","version","env","no-interaction","ansi","no-ansi"]);
      function section(key){if(!sections.has(key)){const group=element("section");group.className="command-form-group";group.dataset.formGroup=key;const heading=element("h3",{arguments:tr("command.argumentsGroup","Arguments"),options:tr("command.optionsGroup","Options"),global:tr("command.globalGroup","Global options")}[key]);heading.className="ui-group-header";heading.id=`command-group-${key}`;group.setAttribute("aria-labelledby",heading.id);group.append(heading);sections.set(key,group);}return sections.get(key);}
      for(const kind of ["arguments","options"])for(const item of command.definition[kind]){
        const key=item.name;if((kind==="arguments"&&key==="command")||["--no-interaction","--ansi","--no-ansi"].includes(`--${key.replace(/^-+/,"")}`))continue;
        const row=element("div");row.className="command-field";const id=`argument-${index++}`;const boolean=kind==="options"&&!item.accept_value,repeated=Boolean(item.is_array||item.is_multiple);const label=element("label",`${key}${item.is_required?" *":""}`);label.htmlFor=id;
        const input=element(repeated?"textarea":"input");input.id=id;input.className=repeated?"ui-textarea":"ui-input";if(!repeated)input.type=boolean?"checkbox":"text";
        if(boolean)input.checked=saved[key]===true;else input.value=Array.isArray(saved[key])?saved[key].join("\n"):typeof saved[key]==="string"?saved[key]:"";
        input.oninput=change;row.append(label,input);const helpIds=[];if(item.description){const help=element("small",item.description);help.id=`${id}-help`;help.className="command-muted";helpIds.push(help.id);row.append(help);}
        const defaultText=element("small",`${repeated?tr("command.onePerLine","One value per line.")+" ":""}${Object.hasOwn(item,"default")?tr("command.defaultValue","Default when omitted: {value}",{value:JSON.stringify(item.default)}):""}`);defaultText.className="command-default";if(defaultText.textContent){defaultText.id=`${id}-default`;helpIds.push(defaultText.id);row.append(defaultText);}if(helpIds.length)input.setAttribute("aria-describedby",helpIds.join(" "));
        let bare=null;if(kind==="options"&&item.accept_value&&!item.is_value_required){const label=element("label");label.className="command-bare-option";bare=element("input");bare.type="checkbox";bare.checked=saved[key]===true;bare.onchange=change;label.append(bare,document.createTextNode(" "+tr("command.enableBare","Enable without a value")));row.append(label);}
        fields.set(key,{input,repeated,bare});section(kind==="arguments"?"arguments":globals.has(key.replace(/^-+/,""))?"global":"options").append(row);
      }
      for(const key of ["arguments","options","global"]){if(sections.has(key))$("arguments").append(sections.get(key));}
      const fixed=element("small",runner==="docker"?tr("command.dockerFixed","Docker runs use --no-interaction and --no-ansi. Cancel stops the Docker client; container PHP may continue."):tr("command.fixedFlags","Captured runs use --no-interaction and --no-ansi."));fixed.className="command-muted";$("arguments").append(fixed);
      window.uiKitControls?.enhance(root);change();
    }
    function output(){if(!job){controls();return;}$("stdout").textContent=job.stdout||"";$("stderr").textContent=job.stderr||"";$("command-status").textContent=`${job.command} · ${job.runner==="docker"?"Docker":tr("command.localPhp","local PHP")}: ${job.running?tr("command.running","Running"):job.cancelled?tr("command.cancelled","Cancelled"):job.status===0?tr("command.succeeded","Succeeded"):tr("command.failed","Failed (exit {status})",{status:job.status})} · ${(job.elapsedMs/1000).toFixed(1)} s${job.truncated?" · "+tr("command.truncated","Output truncated"):""}`;controls();}
    function historyList(){const select=$("command-history");select.replaceChildren(element("option",tr("command.chooseHistory","Choose a previous run…")));select.firstChild.value="";[...(job?[job]:[]),...history].forEach((entry,index)=>{const option=element("option",`${entry.command} — ${entry.running?tr("command.running","Running"):entry.cancelled?tr("command.cancelled","Cancelled"):tr("command.exit","exit {status}",{status:entry.status})}`);option.value=String(index);select.append(option);});}
    function run(terminal=false){if(!selected||!token)return;const id=crypto.randomUUID();send({op:"run",id,command:selected,token,values:values(),terminal,...extraPayload()});}
    $("commands").onkeydown=event=>{if(!["ArrowDown","ArrowUp","Home","End"].includes(event.key))return;const buttons=[...$("commands").querySelectorAll("button")],at=buttons.indexOf(event.target.closest("button"));if(at<0)return;event.preventDefault();const next=event.key==="Home"?0:event.key==="End"?buttons.length-1:Math.max(0,Math.min(buttons.length-1,at+(event.key==="ArrowDown"?1:-1)));buttons[next]?.focus();};
    $("command-docker-target").onclick=()=>send({op:"docker-target"});$("command-runner").onchange=()=>send({op:"runner",runner:$("command-runner").value});$("command-search").oninput=list;$("arguments").onsubmit=event=>event.preventDefault();$("discover").onclick=()=>send({op:"refresh"});$("load-commands").onclick=()=>send({op:"refresh"});$("run").onclick=()=>run();$("terminal").onclick=()=>run(true);$("cancel").onclick=()=>job&&send({op:"cancel",id:job.id});$("copy").onclick=()=>job&&send({op:"copy-output",id:job.id});
    $("rerun").onclick=()=>{if(job){choose(job.command,job.values,job);previewPending=false;send({op:"run",id:crypto.randomUUID(),command:job.command,token,values:job.values,...extraPayload(job)});}};
    $("command-history").onchange=()=>{const index=$("command-history").value;if(index!==""){const entry=[...(job?[job]:[]),...history][Number(index)];if(entry)choose(entry.command,entry.values,entry);}};
    bridge.onMessage(raw=>{if(disposed)return;let reply;try{reply=JSON.parse(raw);}catch{return;}const operation=pending;busy=false;pending="";$("command-error").textContent=operation==="preview"?reply.error||"":"";
      if(Object.hasOwn(reply,"additionalArgv"))supportsExtra=reply.additionalArgv===true;
      if(reply.runner){const nextTarget=reply.runner==="docker"?(reply.targetLabel||""):"";if(runner!==reply.runner||targetLabel!==nextTarget){if(selected&&fields.size)draft.values[selected]=values();runner=reply.runner;targetLabel=nextTarget;draft=draftFor(targetLabel?`${runner}\n${targetLabel}`:runner);selected=draft.selected;fields.clear();}$("command-runner").value=runner;}if(reply.commands){commands=reply.commands;token=reply.token;$("provenance").textContent=token?`${tr("command.count","{count} commands",{count:commands.length})} · ${runner==="docker"?"Docker":tr("command.localPhp","local PHP")}`:tr("command.noneLoaded","No commands loaded");$("provenance").title=token?tr("command.metadataTime","Metadata captured {time}. Refresh to update.",{time:new Date(reply.capturedAt*1000).toLocaleString(window.uiKitI18n?.locale)}):tr("command.loadDefinitions","Load commands to read this project's definitions.");const requested=commands.some(c=>c.name===reply.selectCommand)?reply.selectCommand:selected;choose(commands.some(c=>c.name===requested)?requested:commands.find(c=>c.name==="about")?.name||commands.find(c=>!c.name.startsWith("_"))?.name||commands[0]?.name||"");}
      if(Object.hasOwn(reply,"job")){job=reply.job;if(Object.hasOwn(reply,"history"))history=reply.history||[];output();historyList();remember([...(job?[job.command]:[]),...history.map(entry=>entry.command)]);}
      if(operation==="preview"&&!reply.error&&reply.revision===revision){valid=true;$("command-preview").textContent=tr("command.argvPreview","Project: {root}\n{runner} {argv}",{root:reply.root,runner,argv:JSON.stringify(reply.argv)});}
      if(operation==="preview"&&reply.error){valid=false;$("command-preview").textContent=reply.error;}
      if(reply.terminal){$("command-status").textContent=tr("command.openedTerminal","Opened in terminal.");remember([pendingCommand]);}if(reply.copied)$("command-status").textContent=tr("command.outputCopied","Output copied.");
      controls();if(previewPending){preview();}else if(running()){timer=setTimeout(()=>send({op:"poll",id:job.id}),300);}
    });
    for(const id of ["command-extra-options","command-extra-arguments"]){if($(id))$(id).oninput=change;}
    const languageChanged=()=>{if(disposed)return;list();historyList();output();if(!selected)$("command-name").textContent=tr("command.choose","Choose a command");};
    window.addEventListener("ui-kit-language",languageChanged);
    send({op:"state"});
    return()=>{window.removeEventListener("ui-kit-language",languageChanged);disposed=true;clearTimeout(timer);if(selected)draft.values[selected]=values();saveExtra();};
  }};
})();
