export const appInfo = {
  // learn more about this on https://supertokens.com/docs/thirdpartyemailpassword/appinfo
  appName: "qblok.io",
  // Environment variable needs to be present during build time
  apiDomain: `${
    process.env.QBLOK_BACKEND_API_DOMAIN?.includes("localhost")
      ? "http"
      : "https"
  }://app.${process.env.QBLOK_BACKEND_API_DOMAIN}`,
  websiteDomain: `${
    process.env.QBLOK_WEBSITE_DOMAIN?.includes("localhost") ? "http" : "https"
  }://app.${process.env.QBLOK_WEBSITE_DOMAIN}`,
  apiBasePath: "/api/auth",
  websiteBasePath: "/auth",
};
