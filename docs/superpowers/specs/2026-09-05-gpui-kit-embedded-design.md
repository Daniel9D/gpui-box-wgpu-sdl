# GPUI Kit embedded compatibility

Approved direction: keep engine ownership of SDL3, window, swapchain, device, queue and presentation. Adapt Longbridge gpui-kit revision b586fad393daf683301c634fa4e5d98b4393bbb3 to gpui-box revision 5c7e9eb6de8c8db3e7ff659934166218fb60f9f2.

Keep a source snapshot of the five required upstream crates in vendor/gpui-kit as an independent workspace, with license and provenance. Expose the adapted kit as an optional dependency; default renderer users do not need it. Replace GPUI-family dependencies consistently and remove native application bootstrap from the embedded facade. Fix API differences without removing component behavior.

The engine supplies a separate UI target and composites it over its scene. Existing renderer clear behavior remains explicit. Extend host access for initialization, application updates, and explicit elapsed-time pumping; SDL service synchronization remains an explicit host responsibility.

Validation: compile base and full kit, assert a single GPUI family in the dependency graph, and exercise real kit controls through the external-device host. Document verified functionality and remaining native-platform limitations accurately.
