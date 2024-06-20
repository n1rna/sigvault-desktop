import React from "react";
import { EmailVerificationClaim } from "supertokens-auth-react/recipe/emailverification";
import { useSessionContext } from "supertokens-auth-react/recipe/session";

export default function InvalidClaimHandler(
  props: React.PropsWithChildren<any>,
) {
  let sessionContext = useSessionContext();
  if (sessionContext.loading) {
    return null;
  }

  if (
    sessionContext.invalidClaims.some(
      (i) => i.id === EmailVerificationClaim.id,
    )
  ) {
    // Alternatively you could redirect the user to the email verification screen to trigger the verification email
    // Note: /auth/verify-email is the default email verification path
    // window.location.assign("/auth/verify-email")
    return (
      <div>
        You cannot access this page because your email address is not verified.
      </div>
    );
  }

  // We show the protected route since all claims validators have
  // passed implying that the user has verified their email.
  return <div>{props.children}</div>;
}
