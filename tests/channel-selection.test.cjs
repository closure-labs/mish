'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const helperPath = process.env.MISH_CHANNEL_SELECTION_PATH ||
  path.join(__dirname, '../apps/mish/static/channel-selection.js');
const { chooseChannel } = require(helperPath);
const channels = Object.freeze([
  Object.freeze({ id: 'community' }),
  Object.freeze({ id: 'department-security' }),
  Object.freeze({ id: 'staff' }),
]);

test('community-first catalog retains the initial private staff selection', () => {
  assert.equal(chooseChannel(channels, 'staff'), 'staff');
});

test('blank or missing selection defaults to staff regardless of catalog ordering', () => {
  assert.equal(chooseChannel(channels, ''), 'staff');
  assert.equal(chooseChannel(channels, undefined), 'staff');
});

test('inaccessible prior selection returns to staff', () => {
  assert.equal(chooseChannel(channels, 'department-inaccessible'), 'staff');
});

test('explicit authorized community and department selections are preserved', () => {
  assert.equal(chooseChannel(channels, 'community'), 'community');
  assert.equal(chooseChannel(channels, 'department-security'), 'department-security');
});

test('unavailable staff leaves selection blank instead of implicitly choosing community', () => {
  const withoutStaff = channels.filter(channel => channel.id !== 'staff');
  for (const previous of ['', 'staff', 'department-inaccessible', undefined]) {
    assert.equal(chooseChannel(withoutStaff, previous), '');
  }
  assert.equal(chooseChannel(withoutStaff, 'department-security'), 'department-security');
});

test('legacy empty catalog selects staff without retaining unavailable public selection', () => {
  for (const previous of ['', 'staff', 'community', undefined]) {
    assert.equal(chooseChannel([], previous), 'staff');
  }
});

test('browser export applies the same safe selection policy', () => {
  const window = {};
  vm.runInNewContext(fs.readFileSync(helperPath, 'utf8'), { window });
  assert.equal(typeof window.chooseChannel, 'function');
  assert.equal(window.chooseChannel(channels, ''), 'staff');
  assert.equal(window.chooseChannel([{ id: 'community' }], ''), '');
});
