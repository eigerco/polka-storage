# Getting funds

This document covers getting funds into an account that has been generated externally.

## Setting up your account

In this guide we will be covering getting funds into a externally generated account. The recommended way to generate an account is by using the [polkadot.js wallet extension](https://github.com/polkadot-js/extension). Please make sure to follow the instructions on how to generate a new account if you have not done so already.

[How to create a polkadot account](https://support.polkadot.network/support/solutions/articles/65000098878-how-to-create-a-dot-account)
[Creating a polkadot account (video)](https://www.youtube.com/watch?v=DNU0p5G0Gqc)

## Transferring funds

Make sure to run the local testnet, you can find how to do so in the [local testnet guide](local-testnet.md). Once the local testnet is up and running navigate to the polkadot-js web app interface by going to the [default polkadot.js web interface url](https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:42069).

Under the developer tab, navigate to `sudo`.

![sudo selection](../images/developer-sudo.png)

Once you are in `sudo` you should run the `forceSetBalance` extrinsic in the the `balances` pallet, setting the `Id` field to your generated address and the `newFree` field to the amount in [plancks](../glossary.md#planck), as shown in the image below. If your polkadot.js extension is injected into the polkadot.js web interface it will recognize the injection and you can select the desired account.

Note that the `forceSetBalance` extrinsic does **NOT** top up an account but rather sets the balance to the given amount.

<div class="warning">
The maximum balance for a non-dev account is 0.99... UNITs, 999999999999 plancks. If the amount is above 0.99... UNITs the extrinsic will fail.
</div>

![balance forceSetBalance](../images/force-set-balance.png)

Sign and submit your transaction, the caller will automatically be set to Alice, a dev account.

![sign and submit](../images/sign-and-submit.png)

After the block has been finalized, the balance show up in the generated account under the accounts tab and you are ready to start using the polka-storage chain with your own account.

![account balance](../images/account-balance.png)
