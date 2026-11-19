'use strict';
const $ = (s) => document.querySelector(s);
const esc = (value) => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const pretty = value => esc(JSON.stringify(value, null, 2));
const btn = (action, text, id = '', cls = '') => '<button class="' + cls + '" data-action="' + action + '" data-id="' + esc(id) + '">' + text + '</button>';
const pill = text => '<span class="pill">' + esc(String(text || '').replaceAll('_', ' ')) + '</span>';
const empty = (title, text) => '<div class="empty"><div class="symbol">✦</div><h2>' + esc(title) + '</h2><p class="subtle">' + esc(text) + '</p></div>';
const heading = (tag, title, sub) => '<div class="eyebrow">' + esc(tag) + '</div><h1>' + esc(title) + '</h1><p class="subtle">' + esc(sub) + '</p>';
const field = (name, label, value = '', type = 'text', required = true) => '<label>' + esc(label) + '<input name="' + name + '" type="' + type + '" value="' + esc(value) + '" ' + (required ? 'required' : '') + '></label>';
const select = (name, label, choices, value) => '<label>' + esc(label) + '<select name="' + name + '">' + choices.map(c => { const [id, text] = Array.isArray(c) ? c : [c, c.replaceAll('_',' ')]; return '<option value="' + esc(id) + '" ' + (id === value ? 'selected' : '') + '>' + esc(text) + '</option>'; }).join('') + '</select></label>';
const area = (name, label, value = '', required = false) => '<label>' + esc(label) + '<textarea name="' + name + '" ' + (required ? 'required' : '') + '>' + esc(value) + '</textarea></label>';
let preferences;
try { preferences = JSON.parse(localStorage.getItem('shirori-lab') || '{}'); } catch { preferences = {}; }
const state = { name: /^[a-z0-9_-]{1,40}$/.test(preferences.name) ? preferences.name : 'alice', date: /^\d{4}-\d{2}-\d{2}$/.test(preferences.date) ? preferences.date : '', view: 'daily', dose: null, index: 0, saved: new Set(), cards: new Map(), editorTab: 'cards', pipelineTab: 'sources', cursor: null, busy: false };
function notify(message = '', error = false) { $('#notice').textContent = message; $('#notice').className = error ? 'error' : ''; }
function dateQuery(path) { return path + (state.date ? (path.includes('?') ? '&' : '?') + 'date=' + state.date : ''); }
async function api(path, method = 'GET', body, editor = false) {
  const res = await fetch(path, {method, headers: {'Authorization': 'Bearer dev:' + (editor ? 'editor:lab_editor' : 'reader:' + state.name), ...(body !== undefined ? {'Content-Type': 'application/json'} : {})}, body: body === undefined ? undefined : JSON.stringify(body)});
  const data = res.status === 204 ? null : await res.json();
  if (!res.ok) { const e = data?.error; throw new Error((e?.message || 'Request failed (' + res.status + ')') + (e?.details ? '\n' + JSON.stringify(e.details, null, 2) : '')); }
  return data;
}
const admin = (path, method = 'GET', body) => api('/v1/admin/' + path, method, body, true);
function modal(title, html) { $('#dialog-title').textContent = title; $('#dialog-content').innerHTML = html; $('#dialog-error').textContent = ''; if (!$('#dialog').open) $('#dialog').showModal(); }
function remember(cards) { for (const card of cards) if (card) state.cards.set(card.card_id, card); }
function link(url, label) { try { const u = new URL(url); if (!['http:', 'https:'].includes(u.protocol)) return esc(label); return '<a href="' + esc(u.href) + '" target="_blank" rel="noopener noreferrer">' + esc(label) + ' ↗</a>'; } catch { return esc(label); } }
function cardHtml(card, mode = 'dose') {
  remember([card]);
  return '<article class="card ' + (card.category === 'connection' ? 'connection' : '') + '"><div class="row"><div>' + pill(card.category) + pill(card.evidence_label) + '</div>' + btn('save', state.saved.has(card.card_id) ? '✓ Saved' : '＋ Save', card.card_id + '|' + mode) + '</div><h2>' + esc(card.headline) + '</h2><p class="summary">' + esc(card.summary) + '</p>' + (card.limitations ? '<div class="limitations"><strong>What this doesn’t show</strong><p>' + esc(card.limitations) + '</p></div>' : '') + (card.connection ? '<div class="limitations"><strong>The connection</strong><p>' + esc(card.connection.bridge_sentence) + '</p><small>' + esc(card.connection.left_field) + ' ↔ ' + esc(card.connection.right_field) + '</small></div>' : '') + (card.deeper_explanation ? '<details><summary>A little deeper</summary><p>' + esc(card.deeper_explanation) + '</p></details>' : '') + '<div class="source"><div>' + (card.source ? esc(card.source.title) : 'Built from two connected ideas') + '</div><small>' + esc(card.source_date) + ' · Revision ' + esc(card.revision) + ' · ' + esc(card.field) + '</small>' + btn('sources', 'Explore sources & connections', card.card_id) + '</div></article>';
}
async function loadSavedIds() { state.saved.clear(); let cursor = null; do { const page = await api('/v1/me/saved?limit=100' + (cursor ? '&cursor=' + encodeURIComponent(cursor) : '')); for (const i of page.items) state.saved.add(i.card_id); cursor = page.next_cursor; } while (cursor); }
function setContext() { $('#context').textContent = 'Reader: ' + state.name + ' / ' + (state.date || 'Today · reader’s time zone') + (['editor','pipeline'].includes(state.view) ? ' / Editor: lab_editor' : ''); document.querySelectorAll('[data-view]').forEach(b => b.classList.toggle('active', b.dataset.view === state.view)); }
async function navigate(view) { state.view = view; notify(); setContext(); $('#content').innerHTML = '<p class="subtle">Loading…</p>'; if (view === 'daily') await daily(); else if (view === 'saved') await saved(); else if (view === 'editor') await editorial(); else if (view === 'pipeline') await pipeline(); else if (view === 'settings') await settings(); }
async function daily() { await loadSavedIds(); state.dose = await api(dateQuery('/v1/feed/daily')); state.index = 0; if (state.dose.status === 'completed') done(); else await showDose(); }
async function viewed(card, mode) { await api('/v1/events', 'POST', {event_id: crypto.randomUUID(), event_type: 'card_viewed', card_id: card.card_id, feed_mode: mode}); }
async function showDose() {
  const d = state.dose, item = d.items[state.index];
  if (!item) { $('#content').innerHTML = heading('Daily dose', 'A little quiet today.', 'Your unread catalogue is empty.') + empty('No unread ideas yet', 'Publish more cards in the editorial desk or choose a fresh reader in Test settings.') + btn('complete', 'Finish this empty dose'); return; }
  $('#content').innerHTML = heading('Your daily ritual', 'Make room for a little wonder.', 'A handful of ideas. One new connection. Then, back to your day.') + '<div class="reading"><div class="row"><span class="subtle">' + (state.index + 1) + ' of ' + d.items.length + ' ideas</span><span class="subtle">' + esc(d.dose_date) + '</span></div><div class="progress">' + d.items.map((_, i) => '<span class="' + (i <= state.index ? 'on' : '') + '"></span>').join('') + '</div>' + (item.card ? cardHtml(item.card) : empty('This idea was withdrawn', item.withdrawn?.reason || 'An editor has removed this card.')) + '<div class="row actions"><button data-action="previous" ' + (state.index === 0 ? 'disabled' : '') + '>← Previous</button>' + (state.index < d.items.length - 1 ? btn('next', 'Next idea →', '', 'primary') : btn('complete', 'Finish my dose ✓', '', 'primary')) + '</div><p class="subtle">Use the arrow keys or swipe to move between ideas.</p></div>';
  if (item.card && !item.read) { await viewed(item.card, 'dose'); item.read = true; }
}
function done() { $('#content').innerHTML = heading('Daily dose complete', 'Enough for today. Nicely done.', 'Let those ideas settle. There’s no streak to keep, and no rush to continue.') + '<div class="empty"><div class="symbol">✦</div><h2>A little more connected.</h2><p class="subtle">' + state.dose.items.length + ' ideas in today’s dose. Come back tomorrow for something new.</p><div class="actions" style="justify-content:center">' + btn('discover', 'Still curious? Explore Discover →', '', 'primary') + btn('review-dose', 'Review today’s dose') + '</div></div><p class="subtle">Testing tomorrow? Change the date in Test settings. Read cards will stay out of future doses.</p>'; }
async function discover() {
  const page = await api(dateQuery('/v1/feed/discover?limit=5')); state.view = 'discover'; setContext();
  $('#content').innerHTML = heading('Follow your curiosity', 'A few paths from here.', 'Related ideas first, then a broader view.') + (page.items.length ? '<div class="list">' + page.items.map(i => '<div>' + pill(i.reason) + cardHtml(i.card, 'discover') + btn('mark-read', 'Mark as read', i.card.card_id) + '</div>').join('') + '</div>' : empty('You’re all caught up', 'There are no more unread ideas available right now.')) + '<div class="actions">' + (page.has_more ? btn('discover', 'Next page →', '', 'primary') : '') + btn('back-dose', 'Back to daily dose') + '</div><p class="subtle">In this test view, use “Mark as read” after reading each Discover card. Served pages are not repeated today.</p>';
}
async function saved(cursor = '') {
  const page = await api('/v1/me/saved?limit=20' + (cursor ? '&cursor=' + encodeURIComponent(cursor) : '')); state.cursor = page.next_cursor;
  if (!cursor) $('#content').innerHTML = heading('Your collection', 'Ideas worth keeping.', 'Saved ideas remain here even after you’ve read them.') + '<div class="list" id="saved-list"></div>';
  $('#saved-more')?.remove();
  $('#saved-list').insertAdjacentHTML('beforeend', page.items.map(i => '<div>' + (i.changed_since_saved ? pill('Updated since you saved') : '') + (i.card ? cardHtml(i.card, i.saved_from) : '<div class="panel"><h2>Withdrawn idea</h2><p>' + esc(i.withdrawal?.reason) + '</p>' + btn('save','Remove from Saved',i.card_id + '|dose') + (i.follow_up ? cardHtml(i.follow_up) : '') + '</div>') + '</div>').join(''));
  if (!$('#saved-list').children.length) $('#saved-list').innerHTML = empty('Start a small collection', 'Save an idea from your daily dose and find it here.');
  if (state.cursor) $('#content').insertAdjacentHTML('beforeend', '<div id="saved-more" class="actions">' + btn('more-saved', 'Load more saved ideas') + '</div>');
}
async function sources(id) {
  const [sourceList, connections] = await Promise.all([api('/v1/cards/' + id + '/sources'), api('/v1/cards/' + id + '/connections')]);
  modal('Follow the idea', '<h3>Original sources</h3>' + sourceList.map(s => '<div class="panel"><h3>' + link(s.url, s.title) + '</h3><p class="subtle">' + esc(s.author || s.publisher || '') + '</p>' + pill(s.work_type) + pill(s.rights_status) + '<p class="subtle">' + esc(s.doi_or_isbn || '') + '</p></div>').join('') + '<h3>Connected ideas</h3>' + (connections.length ? connections.map(c => '<div class="panel"><h3>' + esc(c.other_headline) + '</h3><p>' + esc(c.bridge_sentence) + '</p>' + btn('open-card', 'Read this idea', c.other_card_id) + '</div>').join('') : '<p class="subtle">No published connections for this card.</p>'));
}
async function settings() {
  const me = await api('/v1/me');
  $('#content').innerHTML = heading('Test lab', 'Try another reader’s day.', 'Use a new reader name for a fresh history. Dates simulate daily selection without changing your system clock.') + '<div class="settings"><form id="identity-form" class="panel"><h3>Test session</h3>' + field('name','Reader name',state.name) + '<p class="subtle">Lowercase letters, numbers, underscores and hyphens; up to 40 characters. The editor uses a separate identity.</p>' + field('date','Simulated date (blank means today)',state.date,'date',false) + '<div class="actions"><button class="primary">Apply session</button>' + btn('fresh-reader','Create a fresh reader') + '</div></form><form id="preferences-form" class="panel" style="margin-top:18px"><h3>Reader preferences</h3>' + field('timezone','Time zone',me.timezone) + '<label class="check"><input type="checkbox" name="consent" ' + (me.personalisation_consent ? 'checked' : '') + '>Allow personalisation analytics</label><p class="subtle">Read history and saves still work without analytics consent.</p><button class="primary">Save preferences</button></form><div class="actions">' + btn('export','Download my data') + btn('delete-account','Delete this test reader','', 'danger') + '</div></div>';
}
async function editorial() {
  const tabs = ['cards','works','concepts','relations'];
  $('#content').innerHTML = heading('Editorial desk', 'From a source to an idea.', 'Create a draft, check the evidence, then publish. Nothing reaches readers until you approve it.') + '<div class="tabs">' + tabs.map(t => btn('editor-tab',t[0].toUpperCase()+t.slice(1),t,t===state.editorTab?'selected':'')).join('') + '<div id="editor-body">Loading…</div>';
  const target = $('#editor-body'), tab = state.editorTab;
  if (tab === 'cards') {
    const [cards, inventory] = await Promise.all([admin('cards'),admin('inventory')]);
    target.innerHTML = '<div class="metrics"><div class="metric"><strong>' + cards.filter(c=>c.lifecycle_state==='published').length + '</strong><span>Published cards</span></div><div class="metric"><strong>' + cards.filter(c=>c.lifecycle_state==='draft').length + '</strong><span>Drafts</span></div><div class="metric"><strong>' + inventory.usable_connection_cards + '</strong><span>Usable connections · global</span></div></div><div class="actions">' + btn('new-card','＋ Write a card','','primary') + btn('new-connection','＋ Create a connection') + btn('refresh','Refresh') + '</div><div class="list">' + cards.map(c => '<div class="list-item"><div class="row"><div>' + pill(c.lifecycle_state) + pill(c.category) + '</div><span class="subtle">Revision ' + c.latest_revision + '</span></div><h3>' + esc(c.headline) + '</h3><span class="subtle">' + esc(c.field) + ' · ' + esc(c.evidence_label) + '</span><div class="actions">' + btn('edit-card','Preview / revise',c.card_id) + (c.lifecycle_state!=='withdrawn' ? btn('publish','Publish latest',c.card_id) + btn('withdraw','Withdraw',c.card_id,'danger') : '') + '</div></div>').join('') + '</div>';
  } else {
    const path = tab === 'relations' ? 'concept-relations' : tab, rows = await admin(path);
    target.innerHTML = btn('create-resource','＋ Add ' + tab.slice(0,-1), tab, 'primary') + '<div class="list">' + rows.map(r => '<div class="list-item"><h3>' + esc(r.title || r.name || r.source + ' → ' + r.target) + '</h3><p class="subtle">' + esc(r.description || r.rationale || r.author || '') + '</p>' + pill(r.review_status || r.rights_status || r.slug) + (tab==='relations' ? '<div class="actions">' + btn('approve-relation','Approve',r.id) + btn('reject-relation','Reject',r.id) + '</div>' : '') + '<details><summary>Record details</summary><pre>' + pretty(r) + '</pre></details></div>').join('') + (rows.length ? '' : empty('Nothing here yet','Add a record to get started.')) + '</div>';
  }
}
const versionForm = v => field('headline','Headline',v.headline||'') + area('summary','Summary',v.summary||'',true) + '<div class="form-grid">' + select('claim_kind','Claim type',['empirical','historical','reported','interpretive','normative'],v.claim_kind||'empirical') + select('evidence_label','Evidence',['single_study','replicated','review','consensus','contested','cited_argument','reported'],v.evidence_label||'single_study') + field('source_date','Source date',v.source_date||new Date().toISOString().slice(0,10),'date') + '</div>' + area('limitations','What this doesn’t show (required for single study / contested)',v.limitations||'') + area('deeper_explanation','Deeper explanation (optional)',v.deeper_explanation||'');
async function newCard() {
  const [works,concepts] = await Promise.all([admin('works'),admin('concepts')]);
  if (!works.length) throw new Error('Add a source work first in Editorial desk → Works.');
  modal('Write a new card','<form id="card-form"><div class="form-grid">' + select('primary_work_id','Source work',works.map(w=>[w.id,w.title])) + select('category','Category',['research','book','news','evergreen']) + field('field','Field, e.g. biology') + field('concepts','Concept slugs, separated by commas','','text',false) + '</div><p class="subtle">Available concepts: ' + esc(concepts.map(c=>c.slug).join(', ') || 'none yet') + '</p>' + versionForm({}) + '<div class="actions"><button class="primary">Create draft</button></div></form>');
}
async function editCard(id) {
  const detail = await admin('cards/' + id), v = detail.versions[0];
  modal('Preview & revise','<article class="panel"><div>' + pill(detail.card.lifecycle_state) + pill(detail.card.category) + '</div><h2>' + esc(v.headline) + '</h2><p>' + esc(v.summary) + '</p><p class="subtle">Latest revision ' + v.revision + ' · ' + esc(v.evidence_label) + '</p></article>' + (detail.card.lifecycle_state!=='withdrawn' ? '<form id="revision-form" data-id="' + id + '"><h3>Create a new revision</h3>' + versionForm(v) + '<button class="primary">Save new draft revision</button></form>' : '') + '<details><summary>All versions and metadata</summary><pre>' + pretty(detail) + '</pre></details>');
}
async function newConnection() {
  const [cards, relations] = await Promise.all([admin('cards?state=published'), admin('concept-relations')]);
  const plain=cards.filter(c=>c.category!=='connection'), approved=relations.filter(r=>r.review_status==='approved');
  if (plain.length<2 || !approved.length) throw new Error('Publish two ordinary cards and approve a matching concept relation first.');
  modal('Connect two ideas','<form id="connection-form"><p class="subtle">Cards must be from different fields. The approved relationship must match their concepts.</p>' + select('left','First card',plain.map(c=>[c.card_id,c.headline])) + select('right','Second card',plain.map(c=>[c.card_id,c.headline])) + select('relation','Approved relationship',approved.map(r=>[r.id,r.source+' → '+r.target])) + field('bridge','One bridge sentence (maximum 300 characters)') + versionForm({claim_kind:'interpretive',evidence_label:'cited_argument'}) + '<button class="primary">Create connection draft</button></form>');
}
const templates = {
  works: {work_type:'paper',title:'',author:'',canonical_url:'',licence:'link-only',rights_status:'link_only'},
  concepts: {slug:'',name:'',description:''},
  relations: {source:'source-concept-slug',target:'target-concept-slug',relation_type:'analogous_to',rationale:''},
  sources: {name:'',feed_url:'https://example.org/feed.xml',licence:'link-only',rights_status:'link_only',default_work_type:'article',store_excerpts:false,enabled:false},
};
function resourceForm(kind) { modal('Add ' + kind.slice(0,-1), '<form id="resource-form" data-kind="' + kind + '"><p class="subtle">Enter the record below. Source rights are your editorial decision; start feeds disabled until reviewed.</p><label>Record JSON<textarea name="json" class="code" required>' + pretty(templates[kind]) + '</textarea></label><button class="primary">Create record</button></form>'); }
async function pipeline() {
  const tabs=['sources','review','jobs'];
  $('#content').innerHTML = heading('Content pipeline','Keep the ideas moving.','Feeds → review queue → source works → editorial cards.') + '<div class="panel subtle">Polling runs in the separate ingestion worker. Start it with <code>cargo run -p shirori-ingestion</code>. “Poll now” queues a job; refresh to see its result.</div><div class="tabs">' + tabs.map(t=>btn('pipeline-tab', t[0].toUpperCase()+t.slice(1),t,t===state.pipelineTab?'selected':'')).join('') + '</div><div id="pipeline-body">Loading…</div>';
  const tab=state.pipelineTab, rows=await admin(tab==='review'?'ingest/items?status=all&limit=100':tab==='jobs'?'jobs?status=all':tab);
  $('#pipeline-body').innerHTML = '<div class="actions">' + (tab==='sources'?btn('create-resource','＋ Register feed','sources','primary'):'') + btn('refresh','Refresh') + '</div><div class="list">' + rows.map(r=>'<div class="list-item"><div class="row"><h3>' + esc(r.name||r.title||r.kind) + '</h3>' + pill(r.status||(r.enabled?'enabled':'disabled')) + '</div><p class="subtle">' + esc(r.source_name||r.feed_url||r.last_error||'') + '</p>' + (r.changed_since_promotion?pill('Changed since promotion'):'') + '<div class="actions">' + (tab==='sources'?btn('poll','Poll now',r.id)+btn('toggle-source',r.enabled?'Disable':'Enable',r.id+'|'+!r.enabled):tab==='review'?(['new','duplicate'].includes(r.status)?btn('promote','Promote to work',r.id+'|'+r.status)+btn('reject-item','Reject',r.id):'')+(r.changed_since_promotion?btn('ack-change','Acknowledge reviewed change',r.id):''):r.status==='dead'?btn('retry-job','Retry job',r.id):'') + '</div><details><summary>Record details</summary><pre>' + pretty(r) + '</pre></details></div>').join('') + (rows.length?'':empty('The queue is clear','Register a feed and run the worker to bring in source material.')) + '</div>';
}
function reasonForm(action,id,title) { modal(title,'<form id="reason-form" data-action="' + action + '" data-id="' + esc(id) + '">' + area('reason','Reason', '', true) + '<button class="primary">Confirm</button></form>'); }
async function action(name,id,button) {
  if (name==='next'||name==='previous') { state.index += name==='next'?1:-1; return showDose(); }
  if (name==='complete') { await api(dateQuery('/v1/feed/daily/'+state.dose.dose_id+'/complete'),'POST',{}); state.dose.status='completed'; return done(); }
  if (name==='review-dose') { state.index=0; return showDose(); }
  if (name==='back-dose') return navigate('daily');
  if (name==='discover') return discover();
  if (name==='save') { const [card,mode]=id.split('|'); const exists=state.saved.has(card); await api('/v1/me/saved/'+card,exists?'DELETE':'PUT',exists?undefined:{saved_from:mode}); exists?state.saved.delete(card):state.saved.add(card); button.textContent=exists?'＋ Save':'✓ Saved'; if(state.view==='saved') await saved(); return; }
  if (name==='sources') return sources(id);
  if (name==='open-card') { const c=await api('/v1/cards/'+id); return modal('Explore an idea',cardHtml(c,'connection')); }
  if (name==='mark-read') { await viewed(state.cards.get(id),'discover'); button.textContent='✓ Read'; button.dataset.action='read'; return; }
  if (name==='read') return;
  if (name==='more-saved') return saved(state.cursor);
  if (name==='refresh') return navigate(state.view);
  if (name==='editor-tab') {state.editorTab=id;return editorial();}
  if (name==='pipeline-tab') {state.pipelineTab=id;return pipeline();}
  if (name==='new-card') return newCard();
  if (name==='new-connection') return newConnection();
  if (name==='edit-card') return editCard(id);
  if (name==='create-resource') return resourceForm(id);
  if (name==='publish') { await admin('cards/'+id+'/publish','POST',{}); await editorial(); return notify('Published. New daily doses can now include this card.'); }
  if (name==='withdraw') return reasonForm('withdraw',id,'Withdraw card and dependent connections');
  if (name==='reject-item') return reasonForm('reject-item',id,'Reject source item');
  if (name==='ack-change') {if(!confirm('Have you reviewed the source change and corrected affected cards?'))return;await admin('ingest/items/'+id+'/acknowledge-change','POST',{});return pipeline();}
  if (name==='approve-relation'||name==='reject-relation') {await admin('concept-relations/'+id+'/review','POST',{decision:name==='approve-relation'?'approve':'reject'});return editorial();}
  if (name==='poll') {await admin('sources/'+id+'/poll','POST',{});await pipeline();return notify('Poll queued. Run the ingestion worker, then refresh.');}
  if (name==='toggle-source') {const [source,enabled]=id.split('|');await admin('sources/'+source,'PATCH',{enabled:enabled==='true'});return pipeline();}
  if (name==='promote') {const [item,status]=id.split('|'); if(status==='duplicate'&&!confirm('This item is a duplicate. Force promotion into another source work?'))return;await admin('ingest/items/'+item+'/promote','POST',{force:status==='duplicate'});await pipeline();return notify('Source work created. Write a card from Editorial desk → Cards.');}
  if (name==='retry-job') {await admin('jobs/'+id+'/retry','POST',{});return pipeline();}
  if (name==='fresh-reader') {state.name='reader_'+Date.now().toString(36);persist();return navigate('daily');}
  if (name==='export') {const data=await api('/v1/me/export');const url=URL.createObjectURL(new Blob([JSON.stringify(data,null,2)],{type:'application/json'}));const a=document.createElement('a');a.href=url;a.download='shirori-'+state.name+'.json';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);return notify('Reader data exported.');}
  if (name==='delete-account') {if(!confirm('Delete all history and saves for test reader '+state.name+'?'))return;await api('/v1/me','DELETE');state.name='reader_'+Date.now().toString(36);persist();await navigate('settings');return notify('Reader deleted. A new test identity is selected.');}
}
function persist(){localStorage.setItem('shirori-lab',JSON.stringify({name:state.name,date:state.date}));state.dose=null;state.cards.clear();state.saved.clear();}
function versionData(f){return {headline:f.headline,summary:f.summary,claim_kind:f.claim_kind,evidence_label:f.evidence_label,source_date:f.source_date,limitations:f.limitations||null,deeper_explanation:f.deeper_explanation||null};}
async function submit(form){
  const f=Object.fromEntries(new FormData(form));
  if(form.id==='identity-form'){if(!/^[a-z0-9_-]{1,40}$/.test(f.name))throw new Error('Use 1–40 lowercase letters, numbers, underscores or hyphens.');state.name=f.name;state.date=f.date;persist();return navigate('daily');}
  if(form.id==='preferences-form'){await api('/v1/me/preferences','PUT',{timezone:f.timezone,personalisation_consent:f.consent==='on'});return notify('Preferences saved.');}
  if(form.id==='card-form')await admin('cards','POST',{category:f.category,field:f.field,primary_work_id:f.primary_work_id,concepts:f.concepts.split(',').map(s=>s.trim()).filter(Boolean),version:versionData(f)});
  if(form.id==='connection-form')await admin('cards','POST',{category:'connection',field:'cross-field',version:versionData(f),connection:{left_card_id:f.left,right_card_id:f.right,concept_relation_id:f.relation,bridge_sentence:f.bridge}});
  if(form.id==='revision-form')await admin('cards/'+form.dataset.id+'/versions','POST',versionData(f));
  if(form.id==='resource-form'){const kind=form.dataset.kind;await admin(kind==='relations'?'concept-relations':kind,'POST',JSON.parse(f.json));}
  if(form.id==='reason-form'){const a=form.dataset.action;await admin((a==='withdraw'?'cards/':'ingest/items/')+form.dataset.id+'/'+(a==='withdraw'?'withdraw':a==='reject-item'?'reject':'acknowledge-change'),'POST',a==='ack-change'?{note:f.reason}:{reason:f.reason});}
  $('#dialog').close();await navigate(state.view);notify(form.id==='revision-form'?'Revision saved as a draft. Publish latest when ready.':'Saved successfully.');
}
async function run(fn,button){if(state.busy)return;state.busy=true;if(button)button.disabled=true;try{await fn();}catch(e){if($('#dialog').open)$('#dialog-error').textContent=e.message;else notify(e.message,true);}finally{state.busy=false;if(button?.isConnected)button.disabled=false;}}
document.addEventListener('click',event=>{const b=event.target.closest('button');if(!b)return;if(b.id==='close-dialog'){if(!state.busy)$('#dialog').close();return;}if(b.dataset.view)run(()=>navigate(b.dataset.view),b);else if(b.dataset.action)run(()=>action(b.dataset.action,b.dataset.id,b),b);});
document.addEventListener('submit',event=>{event.preventDefault();run(()=>submit(event.target),event.submitter);});
document.addEventListener('keydown',event=>{if(state.view!=='daily'||$('#dialog').open||/INPUT|TEXTAREA|SELECT/.test(event.target.tagName))return;const b=$(event.key==='ArrowRight'?'[data-action="next"]':event.key==='ArrowLeft'?'[data-action="previous"]':'#no-key');if(b&&!b.disabled){event.preventDefault();b.click();}});
let touchX=null;$('#content').addEventListener('touchstart',e=>{touchX=e.touches[0].clientX;},{passive:true});$('#content').addEventListener('touchend',e=>{if(touchX===null||state.view!=='daily')return;const dx=e.changedTouches[0].clientX-touchX;touchX=null;if(Math.abs(dx)>70)$(dx<0?'[data-action="next"]':'[data-action="previous"]')?.click();},{passive:true});
run(async()=>{await api('/health');$('#connection-status').textContent='● LOCAL API CONNECTED';await navigate('daily');});
