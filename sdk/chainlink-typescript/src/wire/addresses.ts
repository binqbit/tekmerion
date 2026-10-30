/** Canonical identities of the optional Chainlink auditor integration. */
import { address, hash, pda, utf8, check, type Network } from '@tekmerion/sdk/wire';

export type ChainlinkNetwork = Network;

export const ADAPTER_IDS = {
  mainnet: 'CLAxRc3x7pW8DZFs8ypRVGVvEJjHeMxvTwn4HcyC7rtm',
  devnet: 'CLAxRc3x7pW8DZFs8ypRVGVvEJjHeMxvTwn4HcyC7rtm',
} as const;

export const profileAddress = (policy: string, network: Network): [string, number] =>
  pda([utf8('chainlink_profile'), address(policy)], ADAPTER_IDS[network]);

export const requestAddress = (
  profile: string,
  caseKey: string,
  network: Network,
): [string, number] =>
  pda([utf8('chainlink_request'), address(profile), address(caseKey)], ADAPTER_IDS[network]);

export const networkDomain = (network: Network): Uint8Array => {
  check(network === 'mainnet' || network === 'devnet', 'Unknown network');
  return hash(utf8(`tekmerion:chainlink:solana-${network}:v1`));
};
