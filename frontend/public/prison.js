console.log("Prison");

// location.origin is Proxy's origin, document.baseURI is target's origin
function isDummy(url) {
  const parsed = new URL(url, document.baseURI);
  return parsed.origin === location.origin && parsed.pathname === "/dummy";
}
function toDummy(url) {
  if (isDummy(url)) {
    return url;
  }

  url = new URL(url, document.baseURI);
  const hash = url.hash;
  url.hash = "";
  newUrl = "/dummy?" + new URLSearchParams({ url });
  if (hash) {
    newUrl += hash;
  }
  return new URL(newUrl, location.origin).href;
}

function interceptMutation(mutations) {
  console.log(mutations);

  for (const mutation of mutations) {
    for (const node of mutation.addedNodes) {
      if (node.tagName === "A") {
        console.log("Intercepting <a>", node);
        node.href = toDummy(node.href);
      } else if (node.tagName === "IFRAME") {
        console.log("Intercepting <iframe>", node);
        node.src = toDummy(node.src);
      }
      if (typeof node.querySelectorAll !== "function") {
        continue;  // text nodes, etc.
      }
      node.querySelectorAll("a").forEach((node) => {
        console.log("Intercepting <a>", node);
        node.href = toDummy(node.href);
      });
      node.querySelectorAll("iframe").forEach((node) => {
        console.log("Intercepting <iframe>", node);
        node.src = toDummy(node.src);
      });
    }
  }
}

// Prevent proxied site from accessing my service worker (GitHub and Netflix would unregister it)
const thisWorker = new URL("/worker.js", location.origin).href;
navigator.serviceWorker.getRegistrations = new Proxy(navigator.serviceWorker.getRegistrations, {
  apply(target, thisArg, args) {
    return target.apply(thisArg, args).then((registrations) => {
      return registrations.filter((registration) => {
        return !registration.active.scriptURL === thisWorker;
      });
    });
  },
});
navigator.serviceWorker.getRegistration = new Proxy(navigator.serviceWorker.getRegistration, {
  apply(target, thisArg, args) {
    return target.apply(thisArg, args).then((registration) => {
      if (registration && registration.active.scriptURL === thisWorker) {
        return undefined;
      }
      return registration;
    });
  },
});

// Overwrite all navigations because 'fetch' event won't trigger for cross-origin requests
const observer = new MutationObserver(interceptMutation);
observer.observe(document.documentElement, {
  childList: true,
  subtree: true,
});
window.open = new Proxy(window.open, {
  apply(target, thisArg, args) {
    console.log("Intercepting window.open", args[0]);
    args[0] = toDummy(args[0]);
    return target.apply(thisArg, args);
  },
});

// Intercept all kinds of navigations (Chrome only)
navigation.addEventListener("navigate", (event) => {
  console.log("Intercepting navigate", event.destination);
  if (!isDummy(event.destination.url)) {
    event.preventDefault();
    location.href = toDummy(event.destination.url);
  }
});

// Intercept history changes, because cross-origin won't work
const pushReplaceState = {
  apply(target, thisArg, args) {
    console.log("Intercepting pushReplaceState", args[2]);
    // These URLs may be relative with /dummy, so need to convert to absolute
    if (isDummy(new URL(args[2], location.origin).href)) {
      args[2] = new URL(args[2], location.origin).href;
    } else {
      args[2] = toDummy(args[2]);
    }
    return target.apply(thisArg, args);
  },
};
history.pushState = new Proxy(history.pushState, pushReplaceState);
history.replaceState = new Proxy(history.replaceState, pushReplaceState);
