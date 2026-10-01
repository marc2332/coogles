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
    this.checked = false
  }
  set innerHTML(contents) {
    this.contents = contents
    if (this.tag === 'section') {
      this.elements = {}
      for (const match of contents.matchAll(/<([a-z]+)[^>]*data-element="([^"]+)"[^>]*>/g)) {
        const element = new Element(match[1])
        if (match[1] === 'select') element.value = 'commits'
        this.elements[match[2]] = element
      }
    }
  }
  get innerHTML() { return this.contents }
  querySelector(selector) { return this.elements[selector.match(/data-element="([^"]+)"/)[1]] }
  setAttribute(name, value) { this.attributes[name] = value }
  append(...children) { this.children.push(...children) }
  replaceChildren(...children) { this.children = children }
  addEventListener(name, callback) { this.listeners[name] = callback }
}

const defaultMetrics = { code_loc: 100, source_code_loc: 70, test_code_loc: 20, comment_loc: 5, doc_loc: 8, example_code_loc: 10 }

function fixture(timestamps, metrics) {
  return {
    repository: '/fixture/first',
    unique_rust_blobs: 3,
    elapsed_seconds: 0.01,
    definitions: 'fixture',
    configuration: {},
    snapshots: timestamps.map((timestamp, index) => ({
      timestamp,
      offset: (index + 1) * 10,
      commit: `commit${index}`,
      metrics: metrics ? metrics[index] : defaultMetrics,
      parse_errors: []
    }))
  }
}

function loadViewer(report, localStorage = { getItem: () => null, setItem: () => {} }) {
  const elements = Object.fromEntries(['report', 'repositories', 'repository-links', 'repository-template', 'download', 'controls', 'axis-mode', 'percentage-mode', 'hide-tables', 'compact-mode', 'hide-points', 'start-month', 'end-month', 'start-month-label', 'end-month-label', 'range-selected', 'date-presets'].map(identifier => [identifier, new Element(identifier)]))
  elements.report.textContent = JSON.stringify(report)
  elements['axis-mode'].value = 'commits'
  const template = readFileSync(join(__dirname, '../src/viewer.html'), 'utf8')
  elements['repository-template'].innerHTML = template.split('<template id="repository-template">')[1].split('</template>')[0]
  const document = {
    getElementById: identifier => elements[identifier],
    createElement: tag => new Element(tag),
    createElementNS: (namespace, tag) => new Element(tag),
    createTextNode: text => text
  }
  const script = template.split('<script>')[1].split('</script>')[0]
  runInNewContext(script, { document, localStorage })
  return { ...elements, ...elements.repositories.children[0].elements, sections: elements.repositories.children }
}

function viewer(timestamps, metrics) { return loadViewer(fixture(timestamps, metrics)) }

function memoryStorage(initial = {}) {
  const values = new Map(Object.entries(initial))
  return {
    getItem: key => values.get(key) ?? null,
    setItem: (key, value) => values.set(key, value)
  }
}

function coordinates(elements, seriesIndex = 0) {
  const polyline = elements.plot.children.filter(child => child.tag === 'polyline')[seriesIndex]
  return polyline.attributes.points.split(' ').map(point => point.split(',').map(Number))
}

function points(elements) { return coordinates(elements).map(point => point[0]) }

function switchMode(elements, mode) {
  elements['axis-mode'].value = mode
  elements['axis-mode'].listeners.change()
}

function percentageMode(elements, enabled) {
  elements['percentage-mode'].checked = enabled
  elements['percentage-mode'].listeners.change()
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

test('one-week and four-week modes change grid spacing without moving samples', () => {
  for (const [mode, spanDays, expectedDates] of [
    ['time-1week', 28, ['2026-01-05', '2026-01-12', '2026-01-19', '2026-01-26', '2026-02-02']],
    ['time-4weeks', 56, ['2026-01-19', '2026-02-16']]
  ]) {
    const elements = viewer([monday + 7 * day, monday + spanDays * day, monday])
    switchMode(elements, mode)
    assert.deepEqual(points(elements), [80, 80 + 7 / spanDays * 860, 940])
    const grids = elements.plot.children.filter(child => child.tag === 'line' && child.attributes['stroke-opacity'] === 0.4)
    assert.deepEqual(grids.map(grid => grid.children[0].textContent), expectedDates)
  }
})

test('controls are grouped in a responsive desktop sidebar', () => {
  const template = readFileSync(join(__dirname, '../src/viewer.html'), 'utf8')
  const sidebar = template.split('<aside id="plot-options"')[1].split('</aside>')[0]
  for (const identifier of ['controls', 'axis-mode', 'percentage-mode', 'hide-tables', 'compact-mode', 'hide-points', 'start-month', 'end-month', 'download']) {
    assert.ok(sidebar.includes(`id="${identifier}"`))
  }
  assert.ok(template.includes('@media (min-width: 1100px)'))
  assert.ok(template.includes('#plot-options { position: fixed;'))
  assert.ok(template.includes('--sidebar-width: clamp(224px, calc((100vw - 1000px) / 2), 672px)'))
  assert.ok(template.includes('width: calc(100% - 2 * var(--sidebar-width) - 80px)'))
  assert.ok(sidebar.includes('value="time-1week"'))
  assert.ok(sidebar.includes('value="time-4weeks"'))
  assert.ok(template.includes('#date-presets { display: flex; flex-wrap: nowrap;'))
  assert.ok(template.includes('overflow-x: auto; padding-bottom: 4px'))
})

test('month range filters all plots and tables using inclusive UTC calendar months', () => {
  const timestamp = value => Date.parse(value) / 1000
  const first = fixture([
    timestamp('2026-03-05T00:00:00Z'),
    timestamp('2026-02-01T00:00:00Z'),
    timestamp('2026-02-01T00:30:00+01:00'),
    timestamp('2025-12-31T23:59:59Z')
  ])
  const second = { ...fixture([timestamp('2026-06-01T00:00:00Z')]), repository: '/fixture/second' }
  const elements = loadViewer({ schema_version: 2, repositories: [first, second] })
  assert.equal(elements['start-month-label'].textContent, '2025-12')
  assert.equal(elements['end-month-label'].textContent, '2026-06')
  elements['start-month'].value = '1'
  elements['end-month'].value = '2'
  elements['start-month'].listeners.input()
  assert.equal(elements['start-month-label'].textContent, '2026-01')
  assert.equal(elements['end-month-label'].textContent, '2026-02')
  assert.equal(elements['start-month'].attributes['aria-valuetext'], '2026-01')
  assert.equal(elements.sections[0].elements.rows.children.length, 2)
  assert.ok(elements.sections[0].elements.summary.textContent.startsWith('2 stops'))
  assert.equal(elements.sections[1].elements.rows.children.length, 0)
  assert.equal(elements.sections[1].elements.plot.children[0].textContent, 'No samples in selected months')
  const points = elements.sections[0].elements.plot.children.filter(child => child.tag === 'circle')
  assert.ok(points[0].children[0].textContent.endsWith('2026-01-31'))
  assert.ok(points[1].children[0].textContent.endsWith('2026-02-01'))
  percentageMode(elements, true)
  switchMode(elements, 'time-1week')
  assert.equal(elements.sections[0].elements.rows.children.length, 2)
  assert.equal(elements.sections[1].elements.rows.children.length, 0)
  elements['start-month'].value = '3'
  elements['start-month'].listeners.input()
  assert.equal(elements['end-month'].value, '3')
  assert.equal(elements.sections[0].elements.rows.children.length, 1)
  elements['end-month'].value = '1'
  elements['end-month'].listeners.input()
  assert.equal(elements['start-month'].value, '1')
  assert.equal(elements.sections[0].elements.rows.children.length, 1)
})

test('date presets select the latest calendar months and restore the full range', () => {
  const timestamp = value => Date.parse(value) / 1000
  const elements = viewer([
    timestamp('2026-12-15T00:00:00Z'),
    timestamp('2026-11-15T00:00:00Z'),
    timestamp('2026-06-15T00:00:00Z'),
    timestamp('2025-12-15T00:00:00Z'),
    timestamp('2025-01-15T00:00:00Z'),
    timestamp('2024-07-15T00:00:00Z')
  ])
  const presets = elements['date-presets'].children
  assert.deepEqual(presets.map(button => button.textContent), ['Last 2 months', 'Last 6 months', 'Last 12 months', 'Last 2 years', 'All time'])
  for (const [index, startLabel, expectedRows] of [[0, '2026-11', 2], [1, '2026-07', 2], [2, '2026-01', 3], [3, '2025-01', 5], [4, '2024-07', 6]]) {
    presets[index].listeners.click()
    assert.equal(elements['start-month-label'].textContent, startLabel)
    assert.equal(elements['end-month-label'].textContent, '2026-12')
    assert.equal(elements.rows.children.length, expectedRows)
  }
  const shortReport = viewer([monday])
  shortReport['date-presets'].children[0].listeners.click()
  assert.equal(shortReport['start-month'].value, '0')
  assert.equal(shortReport.rows.children.length, 1)
})

test('main content stays viewport-centered without overlapping the desktop sidebar', () => {
  const template = readFileSync(join(__dirname, '../src/viewer.html'), 'utf8')
  const desktopBody = template.split('@media (min-width: 1100px)')[1].split('body {')[1].split('}')[0]
  assert.ok(desktopBody.includes('margin: 32px auto'))
  assert.ok(!desktopBody.includes('left:'))
  for (const viewportWidth of [1100, 1400, 1920, 2560, 3840]) {
    const sidebarWidth = Math.max(224, Math.min(672, (viewportWidth - 1000) / 2))
    const contentWidth = Math.min(1100, viewportWidth - 2 * sidebarWidth - 80)
    const contentLeft = (viewportWidth - contentWidth) / 2
    assert.ok(contentWidth > 0)
    assert.ok(contentLeft >= 20 + sidebarWidth + 20)
  }
})

test('month slider is disabled for reports entirely within one month', () => {
  const elements = viewer([monday, monday + day])
  assert.equal(elements['start-month'].disabled, true)
  assert.equal(elements['end-month'].disabled, true)
  assert.equal(elements['start-month-label'].textContent, '2026-01')
  assert.equal(elements['end-month-label'].textContent, '2026-01')
  assert.equal(elements['range-selected'].style.width, '100%')
  assert.equal(elements.rows.children.length, 2)
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

test('percentage mode uses each snapshot total and updates graph, table and tooltips', () => {
  const metrics = [{ ...defaultMetrics, code_loc: 200, source_code_loc: 100 }, defaultMetrics]
  const elements = viewer([monday + day, monday], metrics)
  const rawCoordinates = coordinates(elements)
  percentageMode(elements, true)
  assert.deepEqual(coordinates(elements).map(point => point[1]), [40, 40])
  assert.deepEqual(coordinates(elements, 1).map(point => Math.round(point[1])), [244, 380])
  assert.equal(elements.rows.children[0].children[1].textContent, '100%')
  assert.equal(elements.rows.children[1].children[2].textContent, '50%')
  const circle = elements.plot.children.find(child => child.tag === 'circle')
  assert.equal(circle.children[0].textContent, 'Code (total): 100% · 2026-01-05')
  const totalCheckbox = elements.controls.children[0].children[0]
  totalCheckbox.checked = false
  totalCheckbox.listeners.change()
  assert.deepEqual(coordinates(elements).map(point => Math.round(point[1])), [40, 234])
  totalCheckbox.checked = true
  totalCheckbox.listeners.change()
  switchMode(elements, 'time')
  assert.deepEqual(coordinates(elements).map(point => point[1]), [40, 40])
  percentageMode(elements, false)
  assert.deepEqual(coordinates(elements), rawCoordinates)
  assert.equal(elements.rows.children[1].children[1].textContent, '200')
})

test('hiding total and source rescales axes without changing percentage denominators', () => {
  const elements = viewer([monday])
  percentageMode(elements, true)
  for (const index of [0, 1]) {
    const checkbox = elements.controls.children[index].children[0]
    checkbox.checked = false
    checkbox.listeners.change()
  }
  let labels = elements.plot.children.filter(child => child.tag === 'text' && child.attributes.x === 70)
  assert.equal(labels[labels.length - 1].textContent, '20%')
  assert.equal(elements.rows.children[0].children[3].textContent, '20%')
  assert.deepEqual(coordinates(elements), [[530, 40]])
  const point = elements.plot.children.find(child => child.tag === 'circle')
  assert.equal(point.children[0].textContent, 'Tests: 20% · 2026-01-05')
  percentageMode(elements, false)
  labels = elements.plot.children.filter(child => child.tag === 'text' && child.attributes.x === 70)
  assert.equal(labels[labels.length - 1].textContent, '22')
  for (const label of elements.controls.children) {
    label.children[0].checked = false
    label.children[0].listeners.change()
  }
  percentageMode(elements, true)
  assert.equal(elements.plot.children.filter(child => child.tag === 'polyline').length, 0)
  labels = elements.plot.children.filter(child => child.tag === 'text' && child.attributes.x === 70)
  assert.equal(labels[labels.length - 1].textContent, '1%')
})

test('percentage mode handles empty code and percentages above 100 without clipping', () => {
  const empty = viewer([monday], [{ ...defaultMetrics, code_loc: 0 }])
  percentageMode(empty, true)
  assert.ok(coordinates(empty).flat().every(Number.isFinite))
  assert.equal(empty.rows.children[0].children[1].textContent, 'N/A')
  const large = viewer([monday], [{ ...defaultMetrics, comment_loc: 150 }])
  percentageMode(large, true)
  assert.equal(large.rows.children[0].children[4].textContent, '150%')
  assert.deepEqual(coordinates(large, 3), [[530, 40]])
})

test('multiple repositories have navigation anchors and shared plot controls', () => {
  const first = fixture([monday + 7 * day, monday + 28 * day, monday])
  const second = { ...fixture([monday + 7 * day, monday + 28 * day, monday]), repository: '/fixture/second' }
  const elements = loadViewer({ schema_version: 2, repositories: [first, second] })
  assert.equal(elements.sections.length, 2)
  const links = elements['repository-links'].children.map(entry => entry.children[0])
  assert.deepEqual(links.map(link => link.href), ['#repository-0', '#repository-1'])
  assert.deepEqual(links.map(link => link.textContent), ['first', 'second'])
  assert.deepEqual(elements.sections.map(section => section.id), ['repository-0', 'repository-1'])
  assert.equal(elements.sections[1].elements.heading.textContent, 'second')
  assert.equal(elements.controls.children.length, 6)
  percentageMode(elements, true)
  for (const section of elements.sections) {
    assert.equal(section.elements.rows.children[0].children[1].textContent, '100%')
    assert.equal(section.elements['percentage-mode'], undefined)
    assert.equal(section.elements.controls, undefined)
  }
  switchMode(elements, 'time')
  for (const section of elements.sections) assert.deepEqual(points(section.elements), [80, 295, 940])
  const totalCheckbox = elements.controls.children[0].children[0]
  totalCheckbox.checked = false
  totalCheckbox.listeners.change()
  for (const section of elements.sections) {
    assert.equal(section.elements.plot.children.filter(child => child.tag === 'polyline').length, 5)
  }
  totalCheckbox.checked = true
  totalCheckbox.listeners.change()
  for (const section of elements.sections) {
    assert.equal(section.elements.plot.children.filter(child => child.tag === 'polyline').length, 6)
  }
  for (const hidden of [true, false]) {
    elements['hide-tables'].checked = hidden
    elements['hide-tables'].listeners.change()
    for (const section of elements.sections) assert.equal(section.elements.table.hidden, hidden)
  }
})

test('repository titles link to explicit or inferred GitHub origins', () => {
  const explicit = { ...fixture([monday]), repository: '/tmp/checkout', repository_url: 'https://github.com/owner/explicit' }
  const inferred = { ...fixture([monday]), repository: '/workspace/data/owner/inferred' }
  const elements = loadViewer({ schema_version: 2, repositories: [explicit, inferred] })
  for (const [index, name, url] of [
    [0, 'owner/explicit', 'https://github.com/owner/explicit'],
    [1, 'owner/inferred', 'https://github.com/owner/inferred']
  ]) {
    assert.equal(elements.sections[index].elements.heading.textContent, name)
    assert.equal(elements.sections[index].elements.heading.href, url)
  }
})

test('hidden tables are restored on reload and stay hidden through plot changes', () => {
  const values = new Map()
  const storage = {
    getItem: key => values.get(key) ?? null,
    setItem: (key, value) => values.set(key, value)
  }
  const first = fixture([monday])
  const second = { ...fixture([monday]), repository: '/fixture/second' }
  const report = { schema_version: 2, repositories: [first, second] }
  let elements = loadViewer(report, storage)
  elements['hide-tables'].checked = true
  elements['hide-tables'].listeners.change()
  assert.equal(JSON.parse(values.get('coogles.preferences')).hideTables, true)
  elements = loadViewer(report, storage)
  assert.equal(elements['hide-tables'].checked, true)
  for (const section of elements.sections) assert.equal(section.elements.table.hidden, true)
  percentageMode(elements, true)
  switchMode(elements, 'time')
  for (const section of elements.sections) assert.equal(section.elements.table.hidden, true)
  elements['hide-tables'].checked = false
  elements['hide-tables'].listeners.change()
  elements = loadViewer(report, storage)
  assert.equal(elements['hide-tables'].checked, false)
  for (const section of elements.sections) assert.equal(section.elements.table.hidden, false)
})

test('all plot preferences and manual date ranges are restored together', () => {
  const storage = memoryStorage()
  const report = fixture([monday, monday + 28 * day])
  let elements = loadViewer(report, storage)
  switchMode(elements, 'time-4weeks')
  percentageMode(elements, true)
  elements['compact-mode'].checked = true
  elements['compact-mode'].listeners.change()
  elements['hide-points'].checked = true
  elements['hide-points'].listeners.change()
  elements['hide-tables'].checked = true
  elements['hide-tables'].listeners.change()
  for (const index of [0, 5]) {
    const checkbox = elements.controls.children[index].children[0]
    checkbox.checked = false
    checkbox.listeners.change()
  }
  elements['start-month'].value = '1'
  elements['start-month'].listeners.input()
  elements = loadViewer(report, storage)
  assert.equal(elements['axis-mode'].value, 'time-4weeks')
  assert.equal(elements['percentage-mode'].checked, true)
  assert.equal(elements['compact-mode'].checked, true)
  assert.equal(elements['hide-points'].checked, true)
  assert.equal(elements['hide-tables'].checked, true)
  assert.equal(elements.table.hidden, true)
  assert.equal(elements.sections[0].className, 'compact')
  assert.deepEqual(elements.controls.children.map(label => label.children[0].checked), [false, true, true, true, true, false])
  assert.equal(elements.plot.children.filter(child => child.tag === 'polyline').length, 4)
  assert.equal(elements.rows.children.length, 1)
  assert.equal(elements['start-month-label'].textContent, '2026-02')
  assert.equal(elements['end-month-label'].textContent, '2026-02')
})

test('saved presets follow the latest report month instead of dropping new data', () => {
  const timestamp = value => Date.parse(value) / 1000
  const storage = memoryStorage()
  let elements = loadViewer(fixture([monday, monday + 28 * day]), storage)
  elements['date-presets'].children[0].listeners.click()
  elements = loadViewer(fixture([
    timestamp('2026-06-01T00:00:00Z'), timestamp('2026-05-01T00:00:00Z'), monday
  ]), storage)
  assert.equal(elements['start-month-label'].textContent, '2026-05')
  assert.equal(elements['end-month-label'].textContent, '2026-06')
  assert.equal(elements.rows.children.length, 2)
  elements['date-presets'].children[4].listeners.click()
  elements = loadViewer(fixture([
    timestamp('2026-09-01T00:00:00Z'), timestamp('2026-06-01T00:00:00Z'), monday
  ]), storage)
  assert.equal(elements['end-month-label'].textContent, '2026-09')
  assert.equal(elements.rows.children.length, 3)
})

test('saved date bounds are clamped and malformed preferences fall back safely', () => {
  const january = 2026 * 12
  const storage = memoryStorage({ 'coogles.preferences': JSON.stringify({ dateRange: { start: january - 1, end: january } }) })
  const elements = loadViewer(fixture([monday, monday + 28 * day]), storage)
  assert.equal(elements['start-month-label'].textContent, '2026-01')
  assert.equal(elements['end-month-label'].textContent, '2026-01')
  assert.equal(elements.rows.children.length, 1)
  for (const saved of ['{broken', 'null', JSON.stringify({ axisMode: 'unknown', enabledSeries: 'invalid', dateRange: { start: january - 12, end: january - 10 } })]) {
    const restored = loadViewer(fixture([monday, monday + 28 * day]), memoryStorage({ 'coogles.preferences': saved }))
    assert.equal(restored['axis-mode'].value, 'commits')
    assert.equal(restored.rows.children.length, 2)
    assert.equal(restored.plot.children.filter(child => child.tag === 'polyline').length, 6)
  }
  const legacy = loadViewer(fixture([monday]), memoryStorage({ 'coogles.hideTables': 'true' }))
  assert.equal(legacy['hide-tables'].checked, true)
  assert.equal(legacy.table.hidden, true)
})

test('blocked browser storage does not prevent table toggles', () => {
  const blockedStorage = {
    getItem: () => { throw new Error('blocked') },
    setItem: () => { throw new Error('blocked') }
  }
  const elements = loadViewer(fixture([monday]), blockedStorage)
  assert.equal(elements.table.hidden, false)
  elements['hide-tables'].checked = true
  assert.doesNotThrow(() => elements['hide-tables'].listeners.change())
  assert.equal(elements.table.hidden, true)
})

test('compact mode shrinks every plot and preserves time and percentage controls', () => {
  const first = fixture([monday, monday + 28 * day])
  const second = { ...fixture([monday, monday + 28 * day]), repository: '/fixture/second' }
  const elements = loadViewer({ schema_version: 2, repositories: [first, second] })
  percentageMode(elements, true)
  switchMode(elements, 'time')
  elements['compact-mode'].checked = true
  elements['compact-mode'].listeners.change()
  for (const section of elements.sections) {
    assert.equal(section.className, 'compact')
    assert.equal(section.elements.summary.hidden, true)
    assert.equal(section.elements.plot.attributes.viewBox, '0 0 1000 440')
    assert.deepEqual(coordinates(section.elements).map(point => point[1]), [40, 40])
    assert.deepEqual(coordinates(section.elements, 1).map(point => Math.round(point[1])), [136, 136])
    const gridlines = section.elements.plot.children.filter(child => child.tag === 'line')
    assert.ok(gridlines.every(grid => grid.attributes.y1 <= 360 && grid.attributes.y2 <= 360))
    assert.equal(section.elements.definitions, undefined)
  }
  elements['compact-mode'].checked = false
  elements['compact-mode'].listeners.change()
  for (const section of elements.sections) {
    assert.equal(section.className, '')
    assert.equal(section.elements.summary.hidden, false)
    assert.equal(section.elements.plot.attributes.viewBox, '0 0 1000 800')
    assert.deepEqual(coordinates(section.elements, 1).map(point => Math.round(point[1])), [244, 244])
  }
})

test('hide points removes visible markers globally while preserving lines and hover', () => {
  const first = fixture([monday])
  const second = { ...fixture([monday]), repository: '/fixture/second' }
  const elements = loadViewer({ schema_version: 2, repositories: [first, second] })
  elements['hide-points'].checked = true
  elements['hide-points'].listeners.change()
  for (const section of elements.sections) {
    const points = section.elements.plot.children.filter(child => child.tag === 'circle')
    assert.ok(points.every(point => point.attributes.fill === 'transparent'))
    assert.equal(section.elements.plot.children.filter(child => child.tag === 'polyline').length, 6)
    points[0].listeners.mouseenter()
    assert.equal(section.elements.detail.textContent, 'Code (total): 100 · 2026-01-05')
  }
  elements['hide-points'].checked = false
  elements['hide-points'].listeners.change()
  for (const section of elements.sections) {
    const points = section.elements.plot.children.filter(child => child.tag === 'circle')
    assert.ok(points.every(point => point.attributes.fill !== 'transparent'))
  }
})

test('manifest failures are visible as warnings without preventing plots', () => {
  const report = fixture([monday])
  report.snapshots[0].manifest_errors = ['Cargo.toml: duplicate key']
  const elements = loadViewer(report)
  assert.ok(elements.warning.textContent.includes('1 file/snapshot manifest parse failures'))
  assert.equal(elements.plot.children.filter(child => child.tag === 'polyline').length, 6)
})

test('hover and focus show the selected metric label, value and UTC date', () => {
  const elements = viewer([monday])
  let sourcePoint = elements.plot.children.filter(child => child.tag === 'circle')[1]
  assert.equal(sourcePoint.children[0].textContent, 'Source code: 70 · 2026-01-05')
  assert.equal(sourcePoint.attributes['aria-label'], 'Source code: 70 · 2026-01-05')
  sourcePoint.listeners.mouseenter()
  assert.equal(elements.detail.textContent, 'Source code: 70 · 2026-01-05')
  sourcePoint.listeners.mouseleave()
  assert.equal(elements.detail.textContent, '')
  percentageMode(elements, true)
  sourcePoint = elements.plot.children.filter(child => child.tag === 'circle')[1]
  assert.equal(sourcePoint.children[0].textContent, 'Source code: 70% · 2026-01-05')
  sourcePoint.listeners.focus()
  assert.equal(elements.detail.textContent, 'Source code: 70% · 2026-01-05')
  sourcePoint.listeners.blur()
  assert.equal(elements.detail.textContent, '')
  assert.equal(elements.sections[0].elements['display-mode-note'], undefined)
})
