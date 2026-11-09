// Vercel owns the HTTP listener. Both deployment forms use the same routes and
// atomic Redis coordinator; this adapter never starts the standalone process.
export {default} from '../server/src/function.js';
