(() => {
  const home = 'design/README.md';
  const { pages } = JSON.parse(document.getElementById('reader-data').textContent);
  const byPath = new Map(pages.map(page => [page.path, page]));
  const $ = id => document.getElementById(id);
  const article = $('article');
  const filter = $('filter');
  const mobile = matchMedia('(max-width: 720px)');
  let pagePath;
  let outline = [];

  function route(path, section) {
    return '#/' + path.split('/').map(encodeURIComponent).join('/') + (section ? '#' + encodeURIComponent(section) : '');
  }

  function readLocation() {
    const hash = location.hash.slice(1);
    if (!hash.startsWith('/')) return { path: pagePath || home, section: decode(hash) };
    const [path, section] = hash.slice(1).split('#');
    return { path: decode(path), section: decode(section || '') };
  }

  function decode(value) {
    try { return decodeURIComponent(value); } catch { return value; }
  }

  function link(text, url) {
    const a = document.createElement('a');
    a.href = url;
    a.textContent = text;
    return a;
  }

  function shortTitle(page) {
    return page.title.replace(/^Hello World\s*[:：]\s*/i, '');
  }

  function drawDocuments() {
    const query = filter.value.toLocaleLowerCase().trim();
    const groups = new Map();
    for (const page of pages) {
      if (page.group === '参考资料') continue;
      const matches = query && page.group === '测试用例'
        ? page.headings.filter(heading => heading.text.toLocaleLowerCase().includes(query))
        : [];
      if (!matches.length && !(page.title + ' ' + page.path).toLocaleLowerCase().includes(query)) continue;
      if (!groups.has(page.group)) groups.set(page.group, []);
      groups.get(page.group).push({page, matches});
    }
    const nav = $('documents');
    nav.replaceChildren();
    const order = ['设计导览', '基础主题', '联合主题', '种子清单', '测试用例'];
    const ranked = [...groups].sort(([a], [b]) => (order.includes(a) ? order.indexOf(a) : 9) - (order.includes(b) ? order.indexOf(b) : 9));
    for (const [group, docs] of ranked) {
      const section = document.createElement('section');
      section.className = 'nav-group';
      const label = document.createElement('h3');
      label.textContent = group;
      section.append(label);
      for (const {page, matches} of docs) {
        const title = page.path === home ? '设计首页' : shortTitle(page);
        const a = link(title, route(page.path));
        if (page.path === pagePath) a.setAttribute('aria-current', 'page');
        section.append(a);
        for (const heading of matches) {
          const match = link(heading.text, route(page.path, heading.id));
          match.className = 'case-match';
          section.append(match);
        }
      }
      nav.append(section);
    }
    if (!groups.size) {
      const p = document.createElement('p');
      p.className = 'no-results';
      p.setAttribute('role', 'status');
      p.textContent = '没有匹配的文档';
      nav.append(p);
    }
  }

  function setMenu(open, focusButton = false) {
    const active = mobile.matches && open;
    document.body.classList.toggle('menu-open', active);
    $('menu-button').setAttribute('aria-expanded', String(active));
    $('scrim').hidden = !active;
    $('main').inert = active;
    if (active) filter.focus();
    else if (focusButton) $('menu-button').focus();
  }

  function setCurrentSection() {
    if (!outline.length) return;
    const top = $('main').getBoundingClientRect().top;
    const cutoff = document.querySelector('.masthead').offsetHeight + 40;
    let active = top < 0 ? outline[0] : null;
    for (const record of outline) {
      if (record.el.getBoundingClientRect().top > cutoff) break;
      active = record;
    }
    if (window.scrollY > 0 && window.scrollY + window.innerHeight >= document.documentElement.scrollHeight - 2) {
      const { section } = readLocation();
      const target = outline.find(record => record.el.id === section);
      const targetTop = target?.el.getBoundingClientRect().top;
      active = targetTop >= 0 && targetTop < window.innerHeight ? target : outline.at(-1);
    }
    for (const item of outline) {
      if (active === item) item.link.setAttribute('aria-current', 'location');
      else item.link.removeAttribute('aria-current');
    }
  }

  function drawOutline(page) {
    const nav = $('sections');
    nav.replaceChildren();
    outline = [];
    for (const {id, text, level} of page?.headings ?? []) {
      const el = document.getElementById(id);
      if (level < 2 || !el || !article.contains(el)) continue;
      const a = link(text, route(page.path, id));
      a.dataset.level = level;
      nav.append(a);
      outline.push({el, link: a});
    }
    $('outline').hidden = !outline.length;
    const compact = $('compact-outline');
    compact.hidden = !outline.length;
    compact.open = false;
    compact.querySelector('nav').replaceChildren(...outline.map(record => record.link.cloneNode(true)));
  }

  function render() {
    const {path, section} = readLocation();
    const page = byPath.get(path);
    if (path !== pagePath) {
      pagePath = path;
      setMenu(false);
      drawDocuments();
      const breadcrumb = $('breadcrumb');
      breadcrumb.replaceChildren(link('设计', route(home)));
      const group = document.createElement('span');
      group.textContent = page?.group || '文档';
      breadcrumb.append(group);
      document.title = (page ? shortTitle(page) : '未找到文档') + ' · Normalizer';
      if (page) {
        article.innerHTML = page.html;
        for (const table of article.querySelectorAll('table')) {
          const wrap = document.createElement('div');
          wrap.className = 'table-wrap';
          table.replaceWith(wrap);
          wrap.append(table);
        }
      } else {
        const h1 = document.createElement('h1');
        h1.textContent = '未找到这篇文档';
        article.replaceChildren(h1, link('回到设计首页', route(home)));
      }
      drawOutline(page);
    }
    const target = section && document.getElementById(section);
    if (target && article.contains(target)) target.scrollIntoView();
    else window.scrollTo(0, 0);
    setCurrentSection();
  }

  $('menu-button').addEventListener('click', () => setMenu(!document.body.classList.contains('menu-open')));
  $('menu-close').addEventListener('click', () => setMenu(false, true));
  $('scrim').addEventListener('click', () => setMenu(false, true));
  mobile.addEventListener('change', () => setMenu(false));
  filter.addEventListener('input', drawDocuments);
  $('documents').addEventListener('click', event => {
    if (event.target.closest('a') && mobile.matches) {
      setMenu(false);
      article.focus({preventScroll: true});
    }
  });
  $('compact-outline').addEventListener('click', event => {
    if (event.target.closest('a')) $('compact-outline').open = false;
  });
  document.addEventListener('click', event => {
    const a = event.target.closest('a[href^="#/"]');
    if (a && a.hash === location.hash && !event.ctrlKey && !event.metaKey && !event.shiftKey && !event.altKey) {
      event.preventDefault();
      render();
    }
  });
  document.querySelector('.skip').addEventListener('click', event => {
    event.preventDefault();
    setMenu(false);
    article.focus();
  });
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape') {
      if (document.body.classList.contains('menu-open')) setMenu(false, true);
      else if (document.activeElement === filter) {
        filter.value = '';
        drawDocuments();
        filter.blur();
      }
    }
    if (event.key === '/' && !event.altKey && !event.ctrlKey && !event.metaKey &&
        !event.target.closest('input, textarea, [contenteditable="true"]')) {
      event.preventDefault();
      if (mobile.matches) setMenu(true); else filter.focus();
    }
    if (event.key === 'Tab' && document.body.classList.contains('menu-open')) {
      const focusables = Array.from($('sidebar').querySelectorAll('button, input, a[href]'));
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault(); last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault(); first.focus();
      }
    }
  });
  let pending = false;
  window.addEventListener('scroll', () => {
    if (pending) return;
    pending = true;
    requestAnimationFrame(() => { pending = false; setCurrentSection(); });
  }, {passive: true});
  window.addEventListener('hashchange', render);
  render();
})();
