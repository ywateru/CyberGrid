import assert from 'node:assert/strict';
import {test} from 'node:test';
import {browserLoginFailure, browserStartFailureMessage} from '../src/browser-start-error.ts';

test('existing authorization directs users to the actual recovery controls', () => {
  const message = browserStartFailureMessage('EXISTING_AUTHORIZATION');
  assert.match(message, /恢复本机授权/);
  assert.match(message, /退出并撤销本机授权/);
  assert.notEqual(message, browserLoginFailure);
});

test('unknown and provider errors cannot expose arbitrary native error content', () => {
  for (const error of ['FAILED', 'arbitrary-provider-detail', {code:'EXISTING_AUTHORIZATION', detail:'private-fixture'}, new Error('private-fixture'), null]) {
    assert.equal(browserStartFailureMessage(error), browserLoginFailure);
  }
});
