{
  // Inherit some values
  const info = document.getElementById("swenc-proxy-prison");
  const targetBase = info?.dataset.targetBase;
  const swencOrigin = location.origin === 'null' ? info?.dataset.swencOrigin : location.origin;

  // Keep targetBase in history state
  if (history.state?.targetBase && history.state.targetBase !== targetBase) {
    navigator.serviceWorker.controller.postMessage({
      type: "setBase",
      targetBase: history.state.targetBase
    });
    location.reload();
    throw new Error("Reloading with recovered targetBase:", history.state.targetBase);
  }
  if (location.href !== 'about:blank') history.replaceState({ targetBase }, "", getVisualUrl(location.href));

  // Prevent proxied site from accessing my service worker (GitHub and Netflix would unregister it)
  const thisWorker = new URL("/worker.js", swencOrigin).href;
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
    if (url.origin === swencOrigin && url.pathname.startsWith('/swenc-proxy/url')) {
      // Has embedded URL
      return getVisualUrl(new URLSearchParams(url.search).get('url'));
    } else {
      // Finally show relative URL
      return new URL(url.pathname + url.search + url.hash, swencOrigin).href;
    }
  }
  function toFakeUrl(originalUrl) {
    url = new URL(originalUrl, location.href);
    if (url.origin === swencOrigin) {
      // Same-origin will be captured by Service Worker
      return originalUrl;
    } else {
      // Otherwise rewrite so we can intercept it
      const name = url.pathname.split('/').at(-1) || '';
      return new URL(`/swenc-proxy/url/${name}?` + new URLSearchParams({ url }), swencOrigin).href;
    }
  }

  function patchAnchor(node) {
    // Needed because target="_blank" and user can middle-click
    node.href = toFakeUrl(node.href);
  }
  function patchIframe(node) {
    if (node.src) {
      if (node.src.startsWith("about:")) {
        return;  // Don't patch about:blank
      }
      node.src = toFakeUrl(node.src);
    } else {
      if (node.contentWindow.location.href === "about:blank") {
        // Need a workaround while about:blank doesn't inherit the service worker
        // https://issues.chromium.org/issues/41411856#comment37
        // Luckily, since 135 srcdoc is supported (https://developer.chrome.com/release-notes/135?hl=en#create_service_worker_client_and_inherit_service_worker_controller_for_srcdoc_iframe)
        node.srcdoc = "";
      }

      const script = document.createElement("script");
      script.id = "swenc-proxy-prison";
      script.src = "/swenc-proxy/prison.js";
      script.dataset.targetBase = targetBase;
      script.dataset.swencOrigin = swencOrigin;
      node.contentWindow.document.head.appendChild(script);
    }
  }

  function interceptMutation(mutations) {
    for (const mutation of mutations) {
      for (const node of mutation.addedNodes) {
        // Change direct nodes
        if (node.tagName === "A") {
          patchAnchor(node);
        } else if (node.tagName === "IFRAME") {
          patchIframe(node);
          return;  // Don't need to check child nodes
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
  // If an iframe is altered before the event loop cycles, our patch would overwrite its content
  Element.prototype.appendChild = new Proxy(Element.prototype.appendChild, {
    apply(target, thisArg, args) {
      const result = target.apply(thisArg, args);
      if (args[0].tagName === "IFRAME") {
        patchIframe(args[0]);
      }
      return result;
    },
  });
  window.open = new Proxy(window.open, {
    apply(target, thisArg, args) {
      args[0] = toFakeUrl(args[0]);
      return target.apply(thisArg, args);
    },
  });

  // Intercept all kinds of navigations (Chrome only, https://caniuse.com/mdn-api_navigation_navigate_event)
  navigation.addEventListener("navigate", (event) => {
    // Need to block cross-origin navigations
    if (new URL(event.destination.url).origin !== swencOrigin) {
      event.preventDefault();
      if (event.formData) {
        return;  // will be handled by the form submit event
      }
      location.href = toFakeUrl(event.destination.url);
    }
  });
  document.addEventListener("submit", (event) => {
    if (new URL(event.target.action).origin !== swencOrigin) {
      event.preventDefault();
      event.target.action = toFakeUrl(event.target.action);
      // TODO: try rewriting action with mutationobserver to prevent double submit, if it causes problems
      event.target.submit();
    }
  });

  // Intercept history changes, because won't work cross-origin
  const pushReplaceState = {
    apply(target, thisArg, args) {
      args[2] = toFakeUrl(args[2]);
      return target.apply(thisArg, args);
    },
  };
  history.pushState = new Proxy(history.pushState, pushReplaceState);
  history.replaceState = new Proxy(history.replaceState, pushReplaceState);
}