var K='bud-v1',A=['./','index.html','manifest.json','icon-192.png','icon-512.png'];
self.addEventListener('install',function(e){e.waitUntil(caches.open(K).then(function(c){return c.addAll(A)}));self.skipWaiting()});
self.addEventListener('activate',function(e){e.waitUntil(caches.keys().then(function(ks){return Promise.all(ks.filter(function(k){return k!==K}).map(function(k){return caches.delete(k)}))}))});
self.addEventListener('fetch',function(e){if(e.request.method!=='GET'||e.request.url.indexOf(self.location.origin)!==0)return;e.respondWith(fetch(e.request).then(function(r){var c=r.clone();caches.open(K).then(function(x){x.put(e.request,c)});return r}).catch(function(){return caches.match(e.request)}))});
