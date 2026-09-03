function c(t){if(!t)return null;const n=t.split(/\r?\n/);let s="message";const r=[];return n.forEach(e=>{if(e){if(e.startsWith("event:")){s=e.slice(6).trim()||"message";return}e.startsWith("data:")&&r.push(e.slice(5).trimStart())}}),r.length===0?null:{event:s,data:r.join(`
`)}}async function l(t,n){if(!t.body||typeof t.body.getReader!="function")throw new Error("浏览器不支持流式读取");const s=t.body.getReader(),r=new TextDecoder("utf-8");let e="";for(;;){const{value:d,done:u}=await s.read();if(u)break;e+=r.decode(d,{stream:!0}),e=e.replace(/\r\n/g,`
`);let a=e.indexOf(`

`);for(;a!==-1;){const f=e.slice(0,a);e=e.slice(a+2);const o=c(f);o&&n(o),a=e.indexOf(`

`)}}e+=r.decode();const i=c(e.trim());i&&n(i)}function h(t){if(!t)return null;try{return JSON.parse(t)}catch{return null}}export{l as c,h as s};
