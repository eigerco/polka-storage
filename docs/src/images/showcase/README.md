The tools used for the showcase are [asciinema](https://asciinema.org/), [asciinema_automation](https://github.com/PierreMarchand20/asciinema_automation) and [agg](https://docs.asciinema.org/manual/agg/)

Steps to record a new asciinema session:

1. Install asciinema or use the docker image
2. Check the README.md of the specific showcase for any additional steps
3. Update the `showcase.sh` with the commands you want to showcase
4. Start the asciinema recording

   `asciinema-automation --timeout 300 ./showcase.sh showcase.cast`

5. Convert the `.cast` to `.gif`

   `agg showcase.cast showcase.gif`
