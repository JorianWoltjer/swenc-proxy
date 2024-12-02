console.log("Prison");

function toDummy(url) {
  if (new URL(url, location.origin).pathname === "/dummy") {
    return new URL(url, location.origin).href;
  }

  url = new URL(url, document.baseURI);
  return new URL("/dummy?" + new URLSearchParams({ url }), location.origin).href;
}

function interceptMutation(mutations) {
  console.log(mutations);

  for (const mutation of mutations) {
    for (const node of mutation.addedNodes) {
      if (node.tagName === "A") {
        console.log("Intercepting", node);
        node.href = toDummy(node.href);
      } else if (node.tagName === "IFRAME") {
        console.log("Intercepting", node);
        node.src = toDummy(node.src);
      }
      if (typeof node.querySelectorAll !== "function") {
        continue;  // text nodes, etc.
      }
      node.querySelectorAll("a").forEach((node) => {
        console.log("Intercepting", node);
        node.href = toDummy(node.href);
      });
      node.querySelectorAll("iframe").forEach((node) => {
        console.log("Intercepting", node);
        node.src = toDummy(node.src);
      });
    }
  }
}

const observer = new MutationObserver(interceptMutation);
observer.observe(document.documentElement, {
  childList: true,
  subtree: true,
});

history.pushState = new Proxy(history.pushState, {
  apply(target, thisArg, args) {
    console.log("Intercepting", args[2]);
    args[2] = toDummy(args[2]);
    return target.apply(thisArg, args);
  },
});
history.replaceState = new Proxy(history.replaceState, {
  apply(target, thisArg, args) {
    console.log("Intercepting", args[2]);
    args[2] = toDummy(args[2]);
    return target.apply(thisArg, args);
  },
});
