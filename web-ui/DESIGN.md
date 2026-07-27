# specforge Web UI Design System

Design system for the specforge API documentation web interface, informed by modern API documentation patterns from Stripe, Swagger Editor, Postman, ReadMe, and Redocly.

## Design Principles

- **Light background** (white/off-white) for maximum readability
- **High contrast text** meeting WCAG AA standards (4.5:1 minimum)
- **Subtle borders and shadows** for depth without distraction
- **Professional typography** using system font stacks and monospace for code
- **Consistent spacing** on an 8px grid system
- **Accessible colors** with sufficient contrast ratios
- **Responsive design** that works across all device sizes

---

## Color Palette

### Light Theme (Primary)

| Token | Hex | Usage |
|-------|-----|-------|
| `--bg` | `#ffffff` | Page background |
| `--bg-subtle` | `#f9fafb` | Subtle background (alternating sections, sidebar) |
| `--bg-muted` | `#f3f4f6` | Muted background (code blocks, inputs) |
| `--surface` | `#ffffff` | Card and panel surfaces |
| `--surface-elevated` | `#ffffff` | Elevated surfaces (modals, dropdowns) |
| `--border` | `#e5e7eb` | Default borders |
| `--border-subtle` | `#f3f4f6` | Subtle dividers |
| `--border-strong` | `#d1d5db` | Emphasis borders |
| `--text` | `#111827` | Primary text |
| `--text-secondary` | `#4b5563` | Secondary text |
| `--text-muted` | `#6b7280` | Muted/placeholder text |
| `--text-inverse` | `#ffffff` | Text on accent backgrounds |

### Accent Colors

| Token | Hex | Usage |
|-------|-----|-------|
| `--accent` | `#f97316` | Primary accent (brand orange) |
| `--accent-hover` | `#ea580c` | Accent hover state |
| `--accent-light` | `#fff7ed` | Light accent background |
| `--accent-border` | `#fed7aa` | Accent border |
| `--success` | `#22c55e` | Success states, GET method |
| `--success-bg` | `#f0fdf4` | Success background |
| `--warning` | `#eab308` | Warning states |
| `--warning-bg` | `#fefce8` | Warning background |
| `--error` | `#ef4444` | Error states, DELETE method |
| `--error-bg` | `#fef2f2` | Error background |
| `--info` | `#3b82f6` | Info states, links |
| `--info-bg` | `#eff6ff` | Info background |

### HTTP Method Colors

| Method | Color | Hex |
|--------|-------|-----|
| GET | Green | `#22c55e` |
| POST | Blue | `#3b82f6` |
| PUT | Orange | `#f97316` |
| PATCH | Yellow | `#eab308` |
| DELETE | Red | `#ef4444` |

### Dark Theme

| Token | Hex | Usage |
|-------|-----|-------|
| `--bg` | `#0a0705` | Page background |
| `--bg-subtle` | `#0f0a06` | Subtle background |
| `--bg-muted` | `#1a0f0a` | Muted background |
| `--surface` | `#1c1008` | Card surfaces |
| `--surface-elevated` | `#2a1a0f` | Elevated surfaces |
| `--border` | `#92400e` | Default borders |
| `--border-subtle` | `#78350f` | Subtle dividers |
| `--border-strong` | `#b45309` | Emphasis borders |
| `--text` | `#fef3c7` | Primary text |
| `--text-secondary` | `#fbbf24` | Secondary text |
| `--text-muted` | `#d97706` | Muted text |
| `--accent` | `#fbbf24` | Primary accent (gold) |
| `--accent-hover` | `#f59e0b` | Accent hover |
| `--accent-light` | `#1c1008` | Light accent background |

---

## Typography

### Font Stacks

```css
--font-sans: 'Inter', 'SF Pro Display', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
--font-mono: 'JetBrains Mono', 'SF Mono', 'Fira Code', 'Fira Mono', 'Roboto Mono', Consolas, monospace;
```

### Type Scale

| Token | Size | Line Height | Weight | Usage |
|-------|------|-------------|--------|-------|
| `--text-xs` | 0.75rem (12px) | 1rem | 400 | Labels, captions |
| `--text-sm` | 0.875rem (14px) | 1.25rem | 400 | Secondary text, metadata |
| `--text-base` | 1rem (16px) | 1.5rem | 400 | Body text |
| `--text-lg` | 1.125rem (18px) | 1.75rem | 400 | Large body text |
| `--text-xl` | 1.25rem (20px) | 1.75rem | 500 | Subheadings |
| `--text-2xl` | 1.5rem (24px) | 2rem | 600 | Section headings |
| `--text-3xl` | 1.875rem (30px) | 2.25rem | 600 | Page titles |
| `--text-4xl` | 2.25rem (36px) | 2.5rem | 700 | Hero headings |

### Font Weights

| Token | Weight | Usage |
|-------|--------|-------|
| `--font-normal` | 400 | Body text |
| `--font-medium` | 500 | Emphasis, labels |
| `--font-semibold` | 600 | Headings, buttons |
| `--font-bold` | 700 | Strong emphasis, titles |

### Code Typography

```css
--font-mono-size: 0.875rem (14px);
--font-mono-line-height: 1.625;
--font-mono-weight: 400;
```

---

## Spacing Scale

All spacing values follow an 8px grid system:

| Token | Value | Usage |
|-------|-------|-------|
| `--space-0` | 0 | Reset |
| `--space-1` | 0.25rem (4px) | Tight spacing |
| `--space-2` | 0.5rem (8px) | Small spacing |
| `--space-3` | 0.75rem (12px) | Medium-small spacing |
| `--space-4` | 1rem (16px) | Default spacing |
| `--space-5` | 1.25rem (20px) | Medium spacing |
| `--space-6` | 1.5rem (24px) | Large spacing |
| `--space-8` | 2rem (32px) | Extra large spacing |
| `--space-10` | 2.5rem (40px) | Section spacing |
| `--space-12` | 3rem (48px) | Large section spacing |
| `--space-16` | 4rem (64px) | Page section spacing |

---

## Border Radius

| Token | Value | Usage |
|-------|-------|-------|
| `--radius-none` | 0 | No radius |
| `--radius-sm` | 0.25rem (4px) | Small elements (badges, tags) |
| `--radius-md` | 0.375rem (6px) | Buttons, inputs |
| `--radius-lg` | 0.5rem (8px) | Cards, panels |
| `--radius-xl` | 0.75rem (12px) | Modals, dropdowns |
| `--radius-full` | 9999px | Pills, avatars |

---

## Shadows

| Token | Value | Usage |
|-------|-------|-------|
| `--shadow-sm` | `0 1px 2px 0 rgba(0, 0, 0, 0.05)` | Subtle elevation |
| `--shadow-md` | `0 4px 6px -1px rgba(0, 0, 0, 0.1), 0 2px 4px -2px rgba(0, 0, 0, 0.1)` | Cards, panels |
| `--shadow-lg` | `0 10px 15px -3px rgba(0, 0, 0, 0.1), 0 4px 6px -4px rgba(0, 0, 0, 0.1)` | Dropdowns, modals |
| `--shadow-xl` | `0 20px 25px -5px rgba(0, 0, 0, 0.1), 0 8px 10px -6px rgba(0, 0, 0, 0.1)` | Popovers |

---

## Layout Grid

### Container Widths

| Token | Value | Usage |
|-------|-------|-------|
| `--container-sm` | 640px | Narrow content |
| `--container-md` | 768px | Medium content |
| `--container-lg` | 1024px | Standard content |
| `--container-xl` | 1280px | Wide content |
| `--container-2xl` | 1536px | Full-width content |

### Sidebar Layout

```
+------------------+----------------------------------------+
|                  |                                        |
|    Sidebar       |           Content Area                 |
|    (280px)       |           (flex: 1)                    |
|                  |                                        |
|  - Navigation    |  - Main content                       |
|  - Schema tree   |  - Code blocks                        |
|  - Filters       |  - API reference                      |
|                  |                                        |
+------------------+----------------------------------------+
```

**Sidebar Specifications:**
- Width: 280px (desktop), 100% (mobile overlay)
- Background: `var(--bg-subtle)`
- Border: 1px solid `var(--border)`
- Padding: `var(--space-4)`
- Overflow: `auto` (scrollable)
- Sticky: `position: sticky; top: 0;`

**Content Area:**
- Max-width: 960px
- Padding: `var(--space-8)` (desktop), `var(--space-4)` (mobile)
- Centered with auto margins

### Responsive Breakpoints

| Breakpoint | Width | Layout |
|------------|-------|--------|
| Mobile | < 640px | Single column, sidebar as overlay |
| Tablet | 640px - 1023px | Collapsed sidebar, stacked layout |
| Desktop | >= 1024px | Full sidebar + content |
| Large | >= 1280px | Extended content area |

---

## Component Patterns

### Navigation Bar

```
+------------------------------------------------------------------+
|  [Logo] specforge    [Docs] [API] [GitHub]    [Search] [Theme]  |
+------------------------------------------------------------------+
```

- Height: 64px
- Background: `var(--surface)`
- Border-bottom: 1px solid `var(--border)`
- Padding: 0 `var(--space-6)`
- Position: sticky, top: 0
- Z-index: 50

### Sidebar Navigation

```css
.sidebar {
  width: 280px;
  background: var(--bg-subtle);
  border-right: 1px solid var(--border);
  padding: var(--space-4);
  overflow-y: auto;
  position: sticky;
  top: 64px; /* Below navbar */
  height: calc(100vh - 64px);
}

.sidebar-item {
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-md);
  color: var(--text-secondary);
  font-size: var(--text-sm);
  cursor: pointer;
  transition: all 0.15s ease;
}

.sidebar-item:hover {
  background: var(--bg-muted);
  color: var(--text);
}

.sidebar-item.active {
  background: var(--accent-light);
  color: var(--accent);
  font-weight: var(--font-medium);
}
```

### Cards

```css
.card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: var(--space-6);
  box-shadow: var(--shadow-sm);
  transition: box-shadow 0.2s ease, border-color 0.2s ease;
}

.card:hover {
  box-shadow: var(--shadow-md);
  border-color: var(--border-strong);
}

.card-header {
  font-size: var(--text-lg);
  font-weight: var(--font-semibold);
  color: var(--text);
  margin-bottom: var(--space-2);
}

.card-body {
  font-size: var(--text-base);
  color: var(--text-secondary);
  line-height: 1.6;
}
```

### Buttons

```css
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-4);
  font-size: var(--text-sm);
  font-weight: var(--font-medium);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: all 0.15s ease;
  border: 1px solid transparent;
}

.btn-primary {
  background: var(--accent);
  color: var(--text-inverse);
  border-color: var(--accent);
}

.btn-primary:hover {
  background: var(--accent-hover);
  border-color: var(--accent-hover);
}

.btn-secondary {
  background: transparent;
  color: var(--accent);
  border-color: var(--accent);
}

.btn-secondary:hover {
  background: var(--accent-light);
}

.btn-ghost {
  background: transparent;
  color: var(--text-secondary);
  border-color: transparent;
}

.btn-ghost:hover {
  background: var(--bg-muted);
  color: var(--text);
}

/* Size variants */
.btn-sm {
  padding: var(--space-1) var(--space-3);
  font-size: var(--text-xs);
}

.btn-lg {
  padding: var(--space-3) var(--space-6);
  font-size: var(--text-base);
}
```

### Code Blocks

```css
.code-block {
  background: var(--bg-muted);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: var(--space-4);
  overflow-x: auto;
  font-family: var(--font-mono);
  font-size: var(--font-mono-size);
  line-height: var(--font-mono-line-height);
  position: relative;
}

.code-block-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-3) var(--space-4);
  background: var(--bg-subtle);
  border-bottom: 1px solid var(--border);
  border-radius: var(--radius-lg) var(--radius-lg) 0 0;
}

.code-block-lang {
  font-size: var(--text-xs);
  font-weight: var(--font-medium);
  color: var(--text-muted);
  text-transform: uppercase;
}

.code-block-copy {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  padding: var(--space-1) var(--space-2);
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  color: var(--text-muted);
  cursor: pointer;
  transition: all 0.15s ease;
}

.code-block-copy:hover {
  background: var(--bg-muted);
  color: var(--text);
}

.code-block-copy.copied {
  color: var(--success);
  border-color: var(--success);
}

/* Syntax highlighting (light theme) */
.token-keyword { color: #d946ef; }
.token-string { color: #22c55e; }
.token-number { color: #f97316; }
.token-comment { color: #6b7280; font-style: italic; }
.token-type { color: #3b82f6; }
.token-function { color: #8b5cf6; }
```

### Tabs

```css
.tabs {
  display: flex;
  gap: var(--space-1);
  border-bottom: 1px solid var(--border);
  padding: 0 var(--space-4);
}

.tab {
  padding: var(--space-3) var(--space-4);
  font-size: var(--text-sm);
  font-weight: var(--font-medium);
  color: var(--text-muted);
  border-bottom: 2px solid transparent;
  cursor: pointer;
  transition: all 0.15s ease;
}

.tab:hover {
  color: var(--text);
}

.tab.active {
  color: var(--accent);
  border-bottom-color: var(--accent);
}

/* Tab content */
.tab-content {
  padding: var(--space-4);
}
```

### Method Badges

```css
.method-badge {
  display: inline-flex;
  align-items: center;
  padding: var(--space-1) var(--space-2);
  font-size: var(--text-xs);
  font-weight: var(--font-bold);
  font-family: var(--font-mono);
  border-radius: var(--radius-sm);
  text-transform: uppercase;
}

.method-get {
  background: var(--success-bg);
  color: var(--success);
}

.method-post {
  background: var(--info-bg);
  color: var(--info);
}

.method-put {
  background: var(--accent-light);
  color: var(--accent);
}

.method-patch {
  background: var(--warning-bg);
  color: var(--warning);
}

.method-delete {
  background: var(--error-bg);
  color: var(--error);
}
```

### Input Fields

```css
.input {
  width: 100%;
  padding: var(--space-2) var(--space-3);
  font-size: var(--text-sm);
  color: var(--text);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}

.input:focus {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-light);
}

.input::placeholder {
  color: var(--text-muted);
}

.input-textarea {
  min-height: 120px;
  resize: vertical;
  font-family: var(--font-mono);
}
```

### Tables

```css
.table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--text-sm);
}

.table th {
  text-align: left;
  padding: var(--space-3) var(--space-4);
  font-weight: var(--font-semibold);
  color: var(--text);
  background: var(--bg-subtle);
  border-bottom: 2px solid var(--border);
}

.table td {
  padding: var(--space-3) var(--space-4);
  color: var(--text-secondary);
  border-bottom: 1px solid var(--border);
}

.table tr:hover td {
  background: var(--bg-subtle);
}

.table-code {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--accent);
}
```

### Tooltips

```css
.tooltip {
  position: relative;
}

.tooltip-content {
  position: absolute;
  bottom: 100%;
  left: 50%;
  transform: translateX(-50%);
  padding: var(--space-2) var(--space-3);
  background: var(--text);
  color: var(--text-inverse);
  font-size: var(--text-xs);
  border-radius: var(--radius-md);
  white-space: nowrap;
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.15s ease;
}

.tooltip:hover .tooltip-content {
  opacity: 1;
}
```

---

## Interactive Elements

### Copy Button States

1. **Default**: Border only, muted text
2. **Hover**: Subtle background, darker text
3. **Copied**: Green border and text, "Copied!" label
4. **Reset**: Returns to default after 2 seconds

### Tab Transitions

- Border-bottom animates with `transition: border-color 0.15s ease`
- Color transitions with `transition: color 0.15s ease`
- No scale or transform animations (keep professional)

### Sidebar Interactions

- Hover: Subtle background change (`var(--bg-muted)`)
- Active: Accent background and text color
- Expand/collapse: 150ms ease transition

### Loading States

```css
/* Skeleton loading */
.skeleton {
  background: linear-gradient(
    90deg,
    var(--bg-muted) 25%,
    var(--border) 50%,
    var(--bg-muted) 75%
  );
  background-size: 200% 100%;
  animation: skeleton-loading 1.5s infinite;
  border-radius: var(--radius-md);
}

@keyframes skeleton-loading {
  0% { background-position: 200% 0; }
  100% { background-position: -200% 0; }
}

/* Spinner */
.spinner {
  width: 20px;
  height: 20px;
  border: 2px solid var(--border);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
```

---

## Interactive Element Styles

### Schema Tree Browser

```css
.tree-item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  color: var(--text-secondary);
  cursor: pointer;
  border-radius: var(--radius-md);
  transition: all 0.15s ease;
}

.tree-item:hover {
  background: var(--bg-muted);
  color: var(--text);
}

.tree-item.selected {
  background: var(--accent-light);
  color: var(--accent);
}

.tree-item-type {
  font-size: var(--text-xs);
  color: var(--text-muted);
}
```

### Operation Cards

```css
.operation-card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: var(--space-4);
  margin-bottom: var(--space-4);
  transition: box-shadow 0.2s ease;
}

.operation-card:hover {
  box-shadow: var(--shadow-md);
}

.operation-header {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin-bottom: var(--space-3);
}

.operation-path {
  font-family: var(--font-mono);
  font-size: var(--text-base);
  font-weight: var(--font-semibold);
  color: var(--text);
}

.operation-id {
  font-size: var(--text-sm);
  color: var(--text-muted);
}

.operation-summary {
  font-size: var(--text-sm);
  color: var(--text-secondary);
  margin-top: var(--space-2);
}
```

---

## Accessibility

### Color Contrast Ratios

| Combination | Ratio | WCAG Level |
|-------------|-------|------------|
| Text on Background | 15.4:1 | AAA |
| Secondary Text on Background | 7.5:1 | AAA |
| Accent on Background | 4.8:1 | AA |
| Muted Text on Background | 5.0:1 | AA |
| Text Inverse on Accent | 4.6:1 | AA |

### Focus States

```css
:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

/* For components with custom focus styles */
.input:focus-visible {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-light);
}
```

### Reduced Motion

```css
@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}
```

---

## Dark Mode Implementation

The dark theme uses CSS custom properties that override the light theme values. The toggle uses `data-theme="dark"` on the `<html>` element.

### Key Differences in Dark Mode

1. **Backgrounds**: Shift from white to very dark brown/black tones
2. **Borders**: Use warm brown tones instead of cool grays
3. **Accent**: Shifts from orange (#f97316) to gold (#fbbf24)
4. **Text**: Shifts from cool grays to warm cream tones
5. **Shadows**: Use darker, more subtle shadows

### Dark Mode CSS Structure

```css
:root {
  /* Light theme (default) */
  --bg: #ffffff;
  --text: #111827;
  --accent: #f97316;
  /* ... other light values */
}

[data-theme='dark'] {
  /* Dark theme overrides */
  --bg: #0a0705;
  --text: #fef3c7;
  --accent: #fbbf24;
  /* ... other dark values */
}
```

---

## Animation Guidelines

### Transitions

- **Duration**: 150ms for micro-interactions, 200ms for state changes
- **Easing**: `ease` for standard transitions, `ease-in-out` for complex animations
- **Properties**: Animate `opacity`, `color`, `background-color`, `border-color`, `box-shadow`, `transform`

### What to Animate

- Hover states (color, background, border)
- Focus states (box-shadow, border-color)
- Tab transitions (border-bottom, color)
- Sidebar expand/collapse (width, padding)
- Loading skeletons (background-position)

### What NOT to Animate

- Layout shifts (width, height, position) - use `transform` instead
- Page transitions - keep instant
- Content appearing - keep instant or use opacity fade

---

## Responsive Design Patterns

### Mobile (< 640px)

- Sidebar becomes full-screen overlay
- Navigation collapses to hamburger menu
- Code blocks scroll horizontally
- Reduce padding by 50%
- Stack cards vertically
- Simplify header to logo + menu button

### Tablet (640px - 1023px)

- Sidebar collapses to icon-only or hidden
- Content area takes full width
- Two-column layouts become single column
- Code blocks may wrap or scroll

### Desktop (>= 1024px)

- Full sidebar + content layout
- Three-column layouts possible
- Full typography scale
- Complete component set

### Large (>= 1280px)

- Extended content area (max-width: 960px)
- Additional whitespace
- Larger code blocks

---

## Implementation Notes

### CSS Custom Properties

All design tokens are implemented as CSS custom properties on `:root` for light mode and `[data-theme='dark']` for dark mode. This enables:

1. Runtime theme switching without page reload
2. Easy customization by overriding specific tokens
3. Consistent theming across all components
4. Support for `prefers-color-scheme` media query

### Font Loading

```css
@import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;700&display=swap');
```

Use `font-display: swap` to prevent FOIT (Flash of Invisible Text).

### Browser Support

- Chrome 90+
- Firefox 90+
- Safari 14+
- Edge 90+

### Performance Considerations

1. Use `will-change` sparingly for animated properties
2. Prefer `transform` and `opacity` for animations (GPU-accelerated)
3. Use `content-visibility: auto` for off-screen content
4. Lazy load images and heavy components
5. Use `loading="lazy"` for images below the fold
