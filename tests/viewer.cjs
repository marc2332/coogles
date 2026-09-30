const assert = require('node:assert/strict')
const { readFileSync } = require('node:fs')
const { test } = require('node:test')
const { runInNewContext } = require('node:vm')
const { join } = require('node:path')

class Element {
  constructor(tag) {
    this.tag = tag
    this.attributes = {}
    this.children = []
    this.listeners = {}
    this.style = {}
    this.textContent = ''
  }
  setAttribute(name, value) { this.attributes[name] = value }
  append(...children) { this.children.push(...children) }
  replaceChildren(...children) { this.children = children }
  addEventListener(name, callback) { this.listeners[name] = callback }
}

function viewer(timestamps) {
  const metrics = { code_loc: 100, source_code_loc: 70, test_code_loc: 20, comment_loc: 5, doc_loc: 8, example_code_loc: 10 }
  const report = {
    repository: 'fixture',
    unique_rust_blobs: 3,
    elapsed_seconds: 0.01,
    definitions: 'fixture',
    configuration: {},
    snapshots: timestamps.map((timestamp, index) => ({
      timestamp,
      offset: (index + 1) * 10,
      commit: `commit${index}`,
      metrics,
      parse_errors: []
    }))
  }
  const elements = Object.fromEntries(['report', 'plot', 'axis-mode', 'controls', 'summary', 'definitions', 'warning', 'rows', 'download', 'detail'].map(identifier => [identifier, new Element(identifier)]))
  elements.report.textContent = JSON.stringify(report)
  elements['axis-mode'].value = 'commits'
  const document = {
    getElementById: identifier => elements[identifier],
    createElement: tag => new Element(tag),
    createElementNS: (namespace, tag) => new Element(tag),
    createTextNode: text => text
  }
  const template = readFileSync(join(__dirname, '../src/viewer.html'), 'utf8')
  const script = template.split('<script>')[1].split('</script>')[0]
  runInNewContext(script, { document })
  return elements
}

function points(elements) {
  const polyline = elements.plot.children.find(child => child.tag === 'polyline')
  return polyline.attributes.points.split(' ').map(point => Number(point.split(',')[0]))
}

function switchMode(elements, mode) {
  elements['axis-mode'].value = mode
  elements['axis-mode'].listeners.change()
}

const day = 24 * 60 * 60
const monday = Date.UTC(2026, 0, 5) / 1000

test('time mode spaces samples by date, sorts timestamps and draws fortnightly gridlines', () => {
  const elements = viewer([monday + 7 * day, monday + 28 * day, monday])
  assert.deepEqual(points(elements), [80, 510, 940])
  switchMode(elements, 'time')
  assert.deepEqual(points(elements), [80, 295, 940])
  const gridlines = elements.plot.children.filter(child => child.tag === 'line' && child.attributes['stroke-opacity'] === 0.4)
  assert.deepEqual(gridlines.map(grid => grid.attributes.x1), [80, 510, 940])
  assert.deepEqual(gridlines.map(grid => grid.children[0].textContent), ['2026-01-05', '2026-01-19', '2026-02-02'])
  switchMode(elements, 'commits')
  assert.deepEqual(points(elements), [80, 510, 940])
})

test('single snapshot and identical dates remain centered without invalid coordinates', () => {
  for (const timestamps of [[monday], [monday, monday]]) {
    const elements = viewer(timestamps)
    switchMode(elements, 'time')
    assert.ok(points(elements).every(position => position === 530))
    const labels = elements.plot.children.filter(child => child.tag === 'text' && child.attributes.y === 749)
    assert.equal(labels.length, 1)
    assert.equal(labels[0].textContent, '2026-01-05')
  }
})

test('long histories keep fortnightly grids while limiting date labels', () => {
  const elements = viewer([monday + 365 * day, monday])
  switchMode(elements, 'time')
  const grids = elements.plot.children.filter(child => child.tag === 'line' && child.attributes['stroke-opacity'] === 0.4)
  const labels = elements.plot.children.filter(child => child.tag === 'text' && child.attributes.y === 749)
  assert.equal(grids.length, 27)
  assert.ok(labels.length <= 7)
})
