# THIS FILE IS AUTO-GENERATED. DO NOT MODIFY!!

# Copyright 2020-2023 Tauri Programme within The Commons Conservancy
# SPDX-License-Identifier: Apache-2.0
# SPDX-License-Identifier: MIT

-keep class tech.tinadev.cybergrid.lab.* {
  native <methods>;
}

-keep class tech.tinadev.cybergrid.lab.WryActivity {
  public <init>(...);

  void setWebView(tech.tinadev.cybergrid.lab.RustWebView);
  java.lang.Class getAppClass(...);
  int getId();
  java.lang.String getVersion();
  int startActivity(...);
}

-keep class tech.tinadev.cybergrid.lab.Ipc {
  public <init>(...);

  @android.webkit.JavascriptInterface public <methods>;
}

-keep class tech.tinadev.cybergrid.lab.RustWebView {
  public <init>(...);

  void loadUrlMainThread(...);
  void loadHTMLMainThread(...);
  void evalScript(...);
}

-keep class tech.tinadev.cybergrid.lab.RustWebChromeClient,tech.tinadev.cybergrid.lab.RustWebViewClient {
  public <init>(...);
}
