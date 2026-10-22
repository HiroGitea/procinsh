// Live SPACE host selector: this machine, one remote agent, or all hosts as layers.
import { api } from "../shared/api.js";
import type { HostsResponse } from "../shared/api-types.js";
import { ALL_HOSTS } from "./hosts.js";

/** The `?host=` value of `url`; null selects this machine. */
export function readHostParam(url = location.href): string | null {
  return new URL(url).searchParams.get("host") || null;
}

/** `url` with `?host=` set for a remote host or the merged view, or removed. */
export function hostUrl(host: string | null, url = location.href) {
  const next = new URL(url);
  if (host === null) next.searchParams.delete("host");
  else next.searchParams.set("host", host);
  return next.href;
}

/**
 * Adds a host `<select>` to `container`, refreshed from `/api/hosts`. It stays
 * hidden while this machine is the only host and no other host is selected.
 */
export function mountHostControls(
  container: Element,
  current: () => string | null,
  change: (host: string | null) => void,
  readHosts: () => Promise<HostsResponse> = () =>
    api<HostsResponse>("/api/hosts"),
  interval = 5000,
) {
  const wrapper = document.createElement("div");
  wrapper.id = "host-controls";
  wrapper.className = "demo-controls";
  wrapper.hidden = true;
  const select = document.createElement("select");
  select.id = "host";
  select.setAttribute("aria-label", "Host");
  wrapper.append(select);
  container.prepend(wrapper);
  const option = (value: string, text: string) => {
    const element = document.createElement("option");
    element.value = value;
    element.textContent = text;
    return element;
  };
  async function refresh() {
    let hosts: HostsResponse;
    try {
      hosts = await readHosts();
    } catch {
      return;
    }
    const selected = current();
    const options = hosts.hosts.map((h) =>
      option(
        h.local ? "" : h.name,
        `${h.name}${h.local ? " (local)" : ""}${h.status.state === "offline" ? " (offline)" : ""}`,
      ),
    );
    if (hosts.hosts.length > 1)
      options.push(option(ALL_HOSTS, "All hosts (layers)"));
    if (
      selected !== null &&
      !options.some((element) => element.value === selected)
    )
      options.push(option(selected, `${selected} (unavailable)`));
    select.replaceChildren(...options);
    select.value = selected ?? "";
    wrapper.hidden = hosts.hosts.length <= 1 && selected === null;
  }
  select.onchange = () => change(select.value || null);
  const timer = setInterval(refresh, interval);
  void refresh();
  return {
    refresh,
    dispose() {
      clearInterval(timer);
      wrapper.remove();
    },
  };
}
