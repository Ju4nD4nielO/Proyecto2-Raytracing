# Texturas opcionales

Coloca en esta carpeta cinco imagenes cuadradas en formato PPM binario (`P6`):

- `grass.ppm`: pasto o follaje.
- `stone.ppm`: piedra de santuario con detalles Sheikah.
- `wood.ppm`: madera del arbol y el cofre.
- `water.ppm`: agua estilizada.
- `metal.ppm`: metal de la Espada Maestra y la cerradura.

Se recomienda usar una resolucion de 32x32 o 64x64 y estilo pixel art. Las
imagenes deben poder repetirse en mosaico porque cada cara del cubo usa el rango
UV completo. Si falta un archivo o no es valido, el programa utiliza la textura
procedural correspondiente.

Con ImageMagick se puede convertir una imagen creada por ti:

```powershell
magick grass.png -compress none grass.ppm
```

No es necesario instalar ImageMagick para ejecutar el proyecto; solo sirve para
preparar los recursos antes de entregarlo.
