extern int common_dct(void);
extern int encoder_dct(void);
extern int common_extra(void);
extern int encoder_extra(void);

int main(void) {
  return common_dct() == 17 && encoder_dct() == 29 &&
                 common_extra() == 3 && encoder_extra() == 5
             ? 0
             : 1;
}
