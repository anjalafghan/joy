// joy's motion layer: react-spring on top of the server-rendered pages. Every page works without
// it; this only adds movement. See DESIGN.md, "Motion".
import { mountLayer } from './layer';
import { mountNav } from './nav';
import { initPages } from './pages';
import { initPress } from './press';
import { initReveal } from './reveal';
import { initJar } from './jar';
import { initLists } from './lists';
import { initKept } from './kept';
import { initMonth, initYear } from './scrapbook';

const page = document.body.dataset.page ?? '';
document.documentElement.classList.add('m-on');
initReveal();
initPages();
mountLayer();
if (page !== 'login') {
  mountNav();
  initPress();
}
if (page === 'jar') initJar();
if (page === 'slips' || page === 'kept') initLists();
if (page === 'kept') initKept();
if (page === 'year') initYear();
if (page === 'month') initMonth();
