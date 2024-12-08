console.log("Prison");

// Set variables in the worker
const targetOrigin = document.getElementById("swenc-proxy-prison").dataset.swencProxyOrigin;

// Keep targetOrigin in history state
console.log("State", history.state);
if (history.state?.targetOrigin && history.state.targetOrigin !== targetOrigin) {
  navigator.serviceWorker.controller.postMessage({
    type: "setTargetOrigin",
    origin: history.state.targetOrigin
  });
  location.reload();
  throw new Error("Reloading");
}
history.replaceState({ targetOrigin }, "", getVisualUrl(location.href));

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

function getVisualUrl(url) {
  url = new URL(url, location.href);
  if (url.origin === location.origin && url.pathname === '/swenc-proxy/url') {
    // Has embedded URL
    return getVisualUrl(new URLSearchParams(url.search).get('url'));
  } else {
    // Finally show relative URL
    return new URL(url.pathname + url.search + url.hash, location.origin).href;
  }
}
function toFakeUrl(originalUrl) {
  url = new URL(originalUrl, location.href);
  if (url.origin === location.origin) {
    // Is same-origin, so this will always be captured (Service Worker tag will remember the real origin)
    return originalUrl;
  } else {
    // Otherwise rewrite so we can intercept it
    return new URL("/swenc-proxy/url?" + new URLSearchParams({ url }), location.origin).href;
  }
}

function interceptMutation(mutations) {
  console.log(mutations);

  function patchAnchor(node) {
    console.log("Intercepting <a>", node);
    node.href = toFakeUrl(node.href);
  }
  function patchIframe(node) {
    console.log("Intercepting <iframe>", node);
    if (node.src) {
      node.src = toFakeUrl(node.src);
    }
  }

  for (const mutation of mutations) {
    for (const node of mutation.addedNodes) {
      // Change direct nodes
      if (node.tagName === "A") {
        patchAnchor(node);
      } else if (node.tagName === "IFRAME") {
        patchIframe(node);
      } else if (typeof node.querySelectorAll !== "function") {
        continue;  // Skip text nodes
      }
      // Change child nodes
      node.querySelectorAll("a").forEach((node) => {
        patchAnchor(node);
      });
      node.querySelectorAll("iframe").forEach((node) => {
        patchIframe(node);
      });
    }
  }
}

// Overwrite all navigations because 'fetch' event won't trigger for cross-origin requests
const observer = new MutationObserver(interceptMutation);
observer.observe(document.documentElement, {
  childList: true,
  subtree: true,
});
window.open = new Proxy(window.open, {
  apply(target, thisArg, args) {
    console.log("Intercepting window.open", args[0]);
    args[0] = toFakeUrl(args[0]);
    return target.apply(thisArg, args);
  },
});

// Intercept all kinds of navigations (Chrome only, https://caniuse.com/mdn-api_navigation_navigate_event)
navigation.addEventListener("navigate", (event) => {
  console.log("Intercepting navigate", event.destination);
  // Need to block cross-origin navigations
  if (new URL(event.destination.url).origin !== location.origin) {
    event.preventDefault();
    location.href = toFakeUrl(event.destination.url);
  }
});

// Intercept history changes, because won't work cross-origin
const pushReplaceState = {
  apply(target, thisArg, args) {
    console.log("Intercepting pushReplaceState", args[2]);
    args[2] = toFakeUrl(args[2]);
    return target.apply(thisArg, args);
  },
};
history.pushState = new Proxy(history.pushState, pushReplaceState);
history.replaceState = new Proxy(history.replaceState, pushReplaceState);
