export interface SendModeInfo {
  readonly name: string
  readonly value: number
  readonly description: string
}

export const SEND_MODE_CONSTANTS = {
  0: {
    name: "SendModeRegular",
    description:
      "Ordinary message. Gas fees are deducted from the sending amount. Action phase errors should not be ignored.",
  },
  1: {
    name: "SendModePayFeesSeparately",
    description: "Sender pays transfer fees separately.",
  },
  2: {
    name: "SendModeIgnoreErrors",
    description: "Ignore any errors arising while processing this message during action phase.",
  },
  16: {
    name: "SendModeBounceOnActionFail",
    description:
      "Bounce transaction on action phase failure. Has no effect when SendModeIgnoreErrors (2) is enabled.",
  },
  32: {
    name: "SendModeDestroy",
    description: "Destroy current account if resulting balance is zero.",
  },
  64: {
    name: "SendModeCarryAllRemainingMessageValue",
    description:
      "Carry all remaining value of the inbound message in addition to the value initially indicated in the new message.",
  },
  128: {
    name: "SendModeCarryAllBalance",
    description:
      "Carry all remaining balance of the current smart contract instead of the value originally indicated in the message.",
  },
  1024: {
    name: "SendModeEstimateFeeOnly",
    description: "Do not create an action, only estimate fee.",
  },
} as const

export function parseSendMode(mode: number): SendModeInfo[] {
  const flags: SendModeInfo[] = []
  for (const [value, constant] of Object.entries(SEND_MODE_CONSTANTS)) {
    const flagValue = Number.parseInt(value, 10)
    if (flagValue === 0) continue
    if (mode & flagValue) {
      flags.push({name: constant.name, value: flagValue, description: constant.description})
    }
  }
  if (flags.length === 0 && mode === 0) {
    flags.push({
      name: SEND_MODE_CONSTANTS[0].name,
      value: 0,
      description: SEND_MODE_CONSTANTS[0].description,
    })
  }
  return flags
}
