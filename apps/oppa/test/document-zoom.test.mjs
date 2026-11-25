import assert from 'node:assert/strict';
import { test } from 'node:test';

import { previewZoom } from '../src/lib/document-zoom.ts';

const large = { width: 1200, height: 800 };
const short = { width: 219, height: 150 };

test('thermal receipts default to actual size and fit-window never magnifies', () => {
  assert.equal(previewZoom('actual', large, short, true), 1);
  assert.equal(previewZoom('window', large, short, true), 1);
  assert.equal(previewZoom('width', large, short, true), 4);
});

test('fit-window accommodates long receipts without imposing a clipping minimum', () => {
  const paper = { width: 302, height: 12000 };
  const small = { width: 400, height: 300 };
  const zoom = previewZoom('window', small, paper, true);
  assert.equal(zoom, 252 / 12000);
  assert.ok(paper.height * zoom <= small.height - 48 + 1e-8);
  assert.ok(paper.width * zoom <= small.width - 48 + 1e-8);
});

test('width and manual zoom preserve explicit enlargement; office fit remains independent', () => {
  assert.equal(previewZoom('manual', large, short, true, 2), 2);
  assert.equal(previewZoom('manual', large, short, true, 9), 4);
  assert.equal(previewZoom('manual', large, short, true, 0), 0.1);
  assert.equal(previewZoom('window', large, short, false), 4);
  assert.equal(previewZoom('width', { width: 300, height: 200 }, { width: 302, height: 900 }, true), 252 / 302);
});
