// @ts-check

// Top level checks
var isFramed = false;
try {
  isFramed = window.parent.location !== window.location;

  // If parent's window object is restricted, DOMException is
  // thrown which concludes that the webpage is iframed
} catch (err) {
  isFramed = true;
}

if (!isFramed) {
  function initializeSDK() {
    var contentElement = document.getElementById("payment-link");
    if (contentElement instanceof HTMLDivElement) {
      contentElement.innerHTML = translations.notAllowed;
    } else {
      document.body.innerHTML = translations.notAllowed;
    }
  }
} else {
  /**
   * Trigger - post downloading SDK
   * Uses
   *  - Instantiate SDK
   *  - Create a payment widget
   *  - Decide whether or not to show SDK (based on status)
   **/
  function initializeSDK() {
    // @ts-ignore
    var paymentDetails = window.__PAYMENT_DETAILS;
    var clientSecret = paymentDetails.client_secret;
    var sdkUiRules = paymentDetails.sdk_ui_rules;
    var labelType = paymentDetails.payment_form_label_type;
    var colorIconCardCvcError = paymentDetails.color_icon_card_cvc_error;
    var appearance = {
      variables: {
        colorPrimary: paymentDetails.theme || "rgb(0, 109, 249)",
        fontFamily: "Work Sans, sans-serif",
        fontSizeBase: "16px",
        colorText: "rgb(51, 65, 85)",
        colorTextSecondary: "#334155B3",
        colorPrimaryText: "rgb(51, 65, 85)",
        colorTextPlaceholder: "#33415550",
        borderColor: "#33415550",
        colorBackground: "rgb(255, 255, 255)",
      },
    };
    if (isObject(sdkUiRules)) {
      appearance.rules = sdkUiRules;
    }
    if (labelType !== null && typeof labelType === "string") {
      appearance.labels = labelType;
    }
    if (colorIconCardCvcError !== null && typeof colorIconCardCvcError === "string") {
      appearance.variables.colorIconCardCvcError = colorIconCardCvcError;
    }
    // @ts-ignore
    hyper = window.Hyper(pub_key, {
      isPreloadEnabled: false,
      // TODO: Remove in next deployment
      shouldUseTopRedirection: true,
      redirectionFlags: {
        shouldRemoveBeforeUnloadEvents: true,
        shouldUseTopRedirection: true,
      },
    });
    // @ts-ignore
    widgets = hyper.widgets({
      appearance: appearance,
      clientSecret: clientSecret,
      locale: paymentDetails.locale,
    });
    var type =
      paymentDetails.sdk_layout === "spaced_accordion" ||
        paymentDetails.sdk_layout === "accordion"
        ? "accordion"
        : paymentDetails.sdk_layout;

    var enableSavedPaymentMethod = paymentDetails.enabled_saved_payment_method;
    var hideCardNicknameField = paymentDetails.hide_card_nickname_field;
    var unifiedCheckoutOptions = {
      displaySavedPaymentMethodsCheckbox: enableSavedPaymentMethod,
      displaySavedPaymentMethods: enableSavedPaymentMethod,
      layout: {
        type: type, //accordion , tabs, spaced accordion
        spacedAccordionItems: paymentDetails.sdk_layout === "spaced_accordion",
      },
      branding: "never",
      wallets: {
        walletReturnUrl: paymentDetails.return_url,
        style: {
          theme: "dark",
          type: "default",
          height: 55,
        },
      },
      hideCardNicknameField: hideCardNicknameField,
      showCardFormByDefault: paymentDetails.show_card_form_by_default,
      customMessageForCardTerms: paymentDetails.custom_message_for_card_terms,
    };
    var showCardTerms = paymentDetails.show_card_terms;
    if (showCardTerms !== null && typeof showCardTerms === "string") {
      unifiedCheckoutOptions.terms = {
        card: showCardTerms
      };
    }
    var paymentMethodsHeaderText = paymentDetails.payment_form_header_text;
    if (paymentMethodsHeaderText !== null && typeof paymentMethodsHeaderText === "string") {
      unifiedCheckoutOptions.paymentMethodsHeaderText = paymentMethodsHeaderText;
    }

    unifiedCheckout = widgets.create("payment", unifiedCheckoutOptions);
    mountUnifiedCheckout("#unified-checkout");
    showSDK(paymentDetails.display_sdk_only, paymentDetails.enable_button_only_on_form_ready);

    let shimmer = document.getElementById("payment-details-shimmer");
    shimmer.classList.add("reduce-opacity");

    setTimeout(() => {
      document.body.removeChild(shimmer);
    }, 500);
  }

  /**
   * Use - redirect to /payment_link/status
   */
  function redirectToStatus(paymentDetails) {
    var arr = window.location.pathname.split("/");

    // NOTE - This code preserves '/api' in url for integ and sbx envs
    // e.g. url for integ/sbx - https://integ.hyperswitch.io/api/payment_link/s/merchant_1234/pay_1234?locale=en
    // e.g. url for others - https://abc.dev.com/payment_link/s/merchant_1234/pay_1234?locale=en
    var hasApiInPath = arr.includes("api");
    if (hasApiInPath) {
      arr.splice(0, 4);
      arr.unshift("api", "payment_link", "status");
    } else {
      arr.splice(0, 3);
      arr.unshift("payment_link", "status");
    }

    let returnUrl =
      window.location.origin +
      "/" +
      arr.join("/") +
      "?locale=" +
      paymentDetails.locale;
    try {
      window.top.location.href = returnUrl;

      // Push logs to logs endpoint
    } catch (error) {
      var url = window.location.href;
      var { paymentId, merchantId, attemptId, connector } = parseRoute(url);
      var urlToPost = getEnvRoute(url);
      var message = {
        message:
          "CRITICAL ERROR - Failed to redirect top document. Falling back to redirecting using window.location",
        reason: error.message,
      };
      var log = {
        message,
        url,
        paymentId,
        merchantId,
        attemptId,
        connector,
      };
      postLog(log, urlToPost);

      window.location.href = returnUrl;
    }
  }
}
