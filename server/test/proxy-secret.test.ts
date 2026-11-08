import {test} from 'node:test';
import assert from 'node:assert/strict';
import {proxySecret} from '../src/proxy.js';
test('private proxy secret enforces both bounds in UTF8 bytes',()=>{
 for(const value of [undefined,'','a'.repeat(31),'a'.repeat(257),'é'.repeat(129),'é'.repeat(200)])assert.throws(()=>proxySecret(value),/32\.\.256 bytes/);
 for(const value of ['a'.repeat(32),'a'.repeat(256),'é'.repeat(16),'é'.repeat(128)])assert.equal(proxySecret(value),value);
});
