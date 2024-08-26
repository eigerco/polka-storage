The tools used for the showcase are [asciinema](https://asciinema.org/), [asciinema_automation](https://github.com/PierreMarchand20/asciinema_automation) and [agg](https://docs.asciinema.org/manual/agg/)

Steps to record a new asciinema session:

1. Install asciinema or use the docker image
2. Start the Local Testnet - Polka Storage Parachain
3. Update the `cli_basic.sh` with the commands you want to showcase
4. Start the asciinema recording

   `asciinema-automation --timeout 300 ./cli_basic.sh prod.cast`

5. Convert the `.cast` to `.gif`

   `agg prod.cast prod.gif`
